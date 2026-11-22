package com.teachme.feature.home

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.ActionFailure
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.DueReviews
import com.teachme.core.model.DueTopic
import com.teachme.core.model.FieldIssue
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashKind
import com.teachme.core.model.Folder
import com.teachme.core.model.FolderRepository
import com.teachme.core.model.InMemoryFlashInbox
import com.teachme.core.model.LibraryStateRepository
import com.teachme.core.model.LoadError
import com.teachme.core.model.NotificationPermission
import com.teachme.core.model.NotificationPermissionAsker
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewQueue
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.ReviewReminderPrefs
import com.teachme.core.model.ReviewReminderScheduler
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModelTest {
    private fun folder(
        id: String,
        name: String = id,
        count: Int = 0,
    ) = Folder(id, name, 0, count, Instant.EPOCH, Instant.EPOCH)

    private class FakeFolders(
        var list: ApiResult<List<Folder>> = ApiResult.Success(emptyList()),
    ) : FolderRepository {
        var create: (
            String,
        ) -> ApiResult<Folder> = { ApiResult.Success(Folder("new", it, 0, 0, Instant.EPOCH, Instant.EPOCH)) }
        var rename: (
            String,
            String,
        ) -> ApiResult<Folder> = { id, n -> ApiResult.Success(Folder(id, n, 0, 0, Instant.EPOCH, Instant.EPOCH)) }
        var delete: (String) -> ApiResult<Unit> = { ApiResult.Success(Unit) }
        var listCalls = 0

        /** While set, list calls answer with the data they saw at call time, but only once it completes. */
        var gate: CompletableDeferred<Unit>? = null

        override suspend fun listFolders(): ApiResult<List<Folder>> {
            listCalls++
            val snapshot = list
            gate?.await()
            return snapshot
        }

        override suspend fun getFolder(id: String) = error("unused")

        override suspend fun createFolder(name: String) = create(name)

        override suspend fun renameFolder(
            id: String,
            name: String,
        ) = rename(id, name)

        override suspend fun deleteFolder(id: String) = delete(id)
    }

    private class FakeLibraryState(
        var last: String? = null,
    ) : LibraryStateRepository {
        override suspend fun lastFolderId() = last

        override suspend fun setLastFolderId(id: String?) {
            last = id
        }
    }

    /** What the backend says about the review; `rate` answers with a scripted result. */
    private class FakeReviews : ReviewRepository {
        var due: ApiResult<DueReviews> = ApiResult.Success(DueReviews(emptyList(), 0, 0, null))
        var rateResult: (QueuedReview) -> ApiResult<ReviewCard> = { error("unused") }
        val rated = mutableListOf<QueuedReview>()

        override suspend fun due(
            limit: Int,
            topicId: String?,
        ) = due

        override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> {
            rated += review
            return rateResult(review)
        }

        override suspend fun undo(cardId: String) = error("unused")

        override suspend fun suspend(cardId: String) = error("unused")

        override suspend fun unsuspend(cardId: String) = error("unused")

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ) = error("unused")
    }

    private class MemoryQueue : ReviewQueue {
        private val items = MutableStateFlow<List<QueuedReview>>(emptyList())

        override suspend fun enqueue(review: QueuedReview) {
            items.value = items.value + review
        }

        override suspend fun pending() = items.value

        override suspend fun remove(review: QueuedReview): Boolean {
            val had = items.value.any { it.sameReview(review) }
            items.value = items.value.filterNot { it.sameReview(review) }
            return had
        }

        override fun count(): Flow<Int> = items.map { it.size }
    }

    private class FakePrefs(
        var on: Boolean = true,
    ) : ReviewReminderPrefs {
        var asked = false
        override val enabled: Flow<Boolean> = MutableStateFlow(true)

        override suspend fun isEnabled() = on

        override suspend fun setEnabled(enabled: Boolean) {
            on = enabled
        }

        override suspend fun permissionAsked() = asked

        override suspend fun markPermissionAsked() {
            asked = true
        }
    }

    private class FakePermission(
        var granted: Boolean = false,
    ) : NotificationPermission {
        override fun isGranted() = granted
    }

    private class FakeScheduler : ReviewReminderScheduler {
        var schedules = 0
        var cancels = 0

        override fun schedule() {
            schedules++
        }

        override fun cancel() {
            cancels++
        }
    }

    private val prefs = FakePrefs()
    private val permission = FakePermission()
    private val scheduler = FakeScheduler()
    private val permissionAsker = NotificationPermissionAsker(prefs, permission)
    private val gate = ReviewReminderGate(prefs, permission, scheduler)
    private val reviewsRepo = FakeReviews()
    private val lessonsRepo = FakeResumeLessons()
    private val reviewQueue = MemoryQueue()
    private val inbox = InMemoryFlashInbox()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun vm(
        repo: FakeFolders,
        library: FakeLibraryState = FakeLibraryState(),
        handle: SavedStateHandle = SavedStateHandle(),
    ) = HomeViewModel(
        repo,
        library,
        reviewsRepo,
        lessonsRepo,
        ReviewSyncer(
            reviewsRepo,
            reviewQueue,
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        inbox,
        permissionAsker,
        gate,
        handle,
    )

    @Test
    fun `first load shows folders and selects the first one`() =
        runTest {
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b")))))
            vm.onStart()
            assertTrue(vm.state.value.loading)
            advanceUntilIdle()

            assertEquals(
                listOf("a", "b"),
                vm.state.value.folders
                    ?.map { it.id },
            )
            assertEquals("a", vm.state.value.selectedFolderId)
            assertFalse(vm.state.value.selectionByUser)
        }

    @Test
    fun `the last used folder is selected when it still exists`() =
        runTest {
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b")))), FakeLibraryState("b"))
            vm.onStart()
            advanceUntilIdle()

            assertEquals("b", vm.state.value.selectedFolderId)
        }

    @Test
    fun `the selection survives process death through the saved state`() =
        runTest {
            val handle = SavedStateHandle()
            val folders = FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"))))
            val first = vm(folders, handle = handle)
            first.onStart()
            advanceUntilIdle()
            first.select("b")
            advanceUntilIdle()

            // A new ViewModel built from the same handle starts on the saved selection, before any load.
            val restored = vm(folders, FakeLibraryState("a"), handle)
            assertEquals("b", restored.state.value.selectedFolderId)
            assertTrue(restored.state.value.selectionByUser)
            restored.onStart()
            advanceUntilIdle()
            assertEquals("b", restored.state.value.selectedFolderId)
        }

    @Test
    fun `a user selection is remembered`() =
        runTest {
            val library = FakeLibraryState()
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b")))), library)
            vm.onStart()
            advanceUntilIdle()
            vm.select("b")
            advanceUntilIdle()

            assertEquals("b", library.last)
            assertTrue(vm.state.value.selectionByUser)
        }

    @Test
    fun `first load failure is a full screen error and retry recovers`() =
        runTest {
            val repo = FakeFolders(ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(LoadError.Network, vm.state.value.loadError)
            assertNull(vm.state.value.folders)

            repo.list = ApiResult.Success(listOf(folder("a")))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.loadError)
            assertEquals(
                1,
                vm.state.value.folders
                    ?.size,
            )
        }

    @Test
    fun `unauthorized and server errors are distinguished`() =
        runTest {
            val repo = FakeFolders(ApiResult.Failure(ApiFailure.Unauthorized))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(LoadError.Unauthorized, vm.state.value.loadError)

            repo.list = ApiResult.Failure(ApiFailure.Unexpected(500))
            vm.retry()
            advanceUntilIdle()
            assertEquals(LoadError.Server, vm.state.value.loadError)
        }

    @Test
    fun `a failed refresh with data on screen is a banner and keeps the list`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()

            repo.list = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Other))
            vm.onStart()
            advanceUntilIdle()

            assertTrue(vm.state.value.refreshFailed)
            assertNull(vm.state.value.loadError)
            assertEquals(
                1,
                vm.state.value.folders
                    ?.size,
            )
            vm.dismissRefreshError()
            assertFalse(vm.state.value.refreshFailed)
        }

    @Test
    fun `slow loading is flagged after 8 seconds`() =
        runTest {
            val vm = vm(FakeFolders(ApiResult.Success(emptyList())))
            vm.onStart()
            // The repository answers immediately, so slow loading never shows for a fast call.
            advanceUntilIdle()
            assertFalse(vm.state.value.slowLoading)
            assertEquals(8_000L, SLOW_LOADING_MS)
            advanceTimeBy(SLOW_LOADING_MS)
        }

    @Test
    fun `create adds the folder, selects it and announces it`() =
        runTest {
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            vm.openCreate()

            vm.effects.test {
                vm.create("  Lavoro  ")
                advanceUntilIdle()

                assertEquals(HomeEffect.Toast(HomeMessage.Created("Lavoro")), awaitItem())
                assertEquals(HomeEffect.ScrollTo("new"), awaitItem())
            }
            assertNull(vm.state.value.dialog)
            assertEquals(
                listOf("a", "new"),
                vm.state.value.folders
                    ?.map { it.id },
            )
            assertEquals("new", vm.state.value.selectedFolderId)
        }

    @Test
    fun `create duplicate keeps the dialog open with a field error`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            repo.create = { ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("name", "duplicate")))) }
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreate()
            vm.create("a")
            advanceUntilIdle()

            val dialog = vm.state.value.dialog as HomeDialog.Create
            assertEquals(FieldIssue.Duplicate, dialog.ui.error?.field)
            assertFalse(dialog.ui.submitting)

            vm.onDialogTextEdited()
            assertNull((vm.state.value.dialog as HomeDialog.Create).ui.error)
        }

    @Test
    fun `create network failure shows the panel error and can be retried`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            repo.create = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreate()
            vm.create("x")
            advanceUntilIdle()
            assertEquals(ActionFailure.Network, (vm.state.value.dialog as HomeDialog.Create).ui.error?.action)

            repo.create = { ApiResult.Success(folder("x")) }
            vm.create("x")
            advanceUntilIdle()
            assertNull(vm.state.value.dialog)
        }

    @Test
    fun `rename with the same name closes without a call, a new name updates the row`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a", "Lavoro"))))
            var renameCalls = 0
            repo.rename = { id, n ->
                renameCalls++
                ApiResult.Success(folder(id, n))
            }
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()

            vm.openRename(folder("a", "Lavoro"))
            vm.rename(" Lavoro ")
            advanceUntilIdle()
            assertEquals(0, renameCalls)
            assertNull(vm.state.value.dialog)

            vm.openRename(folder("a", "Lavoro"))
            vm.rename("Studio")
            advanceUntilIdle()
            assertEquals(
                "Studio",
                vm.state.value.folders
                    ?.single()
                    ?.name,
            )
        }

    @Test
    fun `rename of a folder deleted elsewhere closes the dialog and reloads`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"))))
            repo.rename = { _, _ -> ApiResult.Failure(ApiFailure.NotFound) }
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            val calls = repo.listCalls
            vm.openRename(folder("a"))

            vm.effects.test {
                vm.rename("zzz")
                advanceUntilIdle()
                assertEquals(HomeEffect.Toast(HomeMessage.FolderGone), awaitItem())
                assertEquals(HomeEffect.FolderRemoved("a"), awaitItem())
            }
            assertNull(vm.state.value.dialog)
            assertEquals(calls + 1, repo.listCalls)
        }

    @Test
    fun `delete removes the row, moves the selection and treats 404 as success`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"), folder("c"))))
            repo.delete = { ApiResult.Failure(ApiFailure.NotFound) }
            val vm = vm(repo, FakeLibraryState("b"))
            vm.onStart()
            advanceUntilIdle()
            assertEquals("b", vm.state.value.selectedFolderId)

            vm.openDelete(folder("b", "B"))
            vm.effects.test {
                vm.confirmDelete()
                advanceUntilIdle()
                assertEquals(HomeEffect.Toast(HomeMessage.Deleted("B")), awaitItem())
                assertEquals(HomeEffect.FolderRemoved("b"), awaitItem())
            }
            assertEquals(
                listOf("a", "c"),
                vm.state.value.folders
                    ?.map { it.id },
            )
            assertEquals("c", vm.state.value.selectedFolderId)
        }

    @Test
    fun `delete failure keeps the dialog open`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            repo.delete = { ApiResult.Failure(ApiFailure.Unexpected(500)) }
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            vm.openDelete(folder("a"))
            vm.confirmDelete()
            advanceUntilIdle()

            val dialog = vm.state.value.dialog as HomeDialog.Delete
            assertEquals(ActionFailure.Unexpected, dialog.ui.error?.action)
            assertEquals(
                1,
                vm.state.value.folders
                    ?.size,
            )
        }

    @Test
    fun `a dialog cannot be dismissed while submitting`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreate()
            vm.create("x")
            // The request has not completed yet (test dispatcher is paused).
            vm.dismissDialog()
            assertTrue(vm.state.value.dialog != null)
            advanceUntilIdle()
        }

    @Test
    fun `a flash from the folder screen becomes a toast on start`() =
        runTest {
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            inbox.post(Flash(FlashKind.FolderDeleted, "Lavoro"))

            vm.effects.test {
                vm.onStart()
                advanceUntilIdle()
                assertEquals(HomeEffect.Toast(HomeMessage.Deleted("Lavoro")), awaitItem())
            }
        }

    @Test
    fun `folder removed from the detail pane moves the selection to the neighbour`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"))))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()
            repo.list = ApiResult.Success(listOf(folder("b")))

            vm.folderRemoved("a", HomeMessage.FolderGone)
            advanceUntilIdle()

            assertEquals("b", vm.state.value.selectedFolderId)
        }

    @Test
    fun `a refresh started before a create does not wipe the new folder`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"))))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()

            val gate = CompletableDeferred<Unit>()
            repo.gate = gate
            vm.retry()
            advanceUntilIdle()
            vm.openCreate()
            vm.create("Nuova")
            advanceUntilIdle()
            assertEquals(
                listOf("a", "new"),
                vm.state.value.folders
                    ?.map { it.id },
            )

            gate.complete(Unit)
            advanceUntilIdle()

            assertEquals(
                listOf("a", "new"),
                vm.state.value.folders
                    ?.map { it.id },
            )
            assertEquals("new", vm.state.value.selectedFolderId)
        }

    @Test
    fun `a refresh started before a delete does not bring the folder back`() =
        runTest {
            val repo = FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"))))
            val vm = vm(repo)
            vm.onStart()
            advanceUntilIdle()

            val gate = CompletableDeferred<Unit>()
            repo.gate = gate
            vm.retry()
            advanceUntilIdle()
            vm.openDelete(folder("b"))
            vm.confirmDelete()
            advanceUntilIdle()
            gate.complete(Unit)
            advanceUntilIdle()

            assertEquals(
                listOf("a"),
                vm.state.value.folders
                    ?.map { it.id },
            )
        }

    @Test
    fun `the selection moved by a delete is persisted`() =
        runTest {
            val library = FakeLibraryState("b")
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a"), folder("b"), folder("c")))), library)
            vm.onStart()
            advanceUntilIdle()

            vm.openDelete(folder("b"))
            vm.confirmDelete()
            advanceUntilIdle()

            assertEquals("c", vm.state.value.selectedFolderId)
            assertEquals("c", library.last)
        }

    // --- Ripasso di oggi -------------------------------------------------------------------------

    private fun due(
        count: Int,
        next: Instant? = null,
    ) = ApiResult.Success(DueReviews(emptyList(), count, 0, next))

    @Test
    fun `the review card shows how many cards are due`() =
        runTest {
            reviewsRepo.due = due(12)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Due(12), vm.state.value.review)
        }

    @Test
    fun `nothing due but a card coming is a quiet line, and no card at all shows nothing`() =
        runTest {
            val next = Instant.parse("2026-11-11T09:00:00Z")
            reviewsRepo.due = due(0, next)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Upcoming(next), vm.state.value.review)
            reviewsRepo.due = due(0, null)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Hidden, vm.state.value.review)
        }

    @Test
    fun `cards graded but still queued are not counted as due`() =
        runTest {
            reviewsRepo.rateResult = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            reviewQueue.enqueue(QueuedReview("c1", "good", 1L, 0))
            reviewQueue.enqueue(QueuedReview("c2", "good", 1L, 0))
            reviewsRepo.due = due(5)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Due(3), vm.state.value.review)
        }

    @Test
    fun `the queue is sent before what is due is counted`() =
        runTest {
            reviewsRepo.rateResult = { ApiResult.Failure(ApiFailure.Conflict) }
            reviewQueue.enqueue(QueuedReview("c1", "good", 1L, 0))
            reviewsRepo.due = due(2)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(1, reviewsRepo.rated.size)
            assertTrue(reviewQueue.pending().isEmpty())
            assertEquals(HomeReview.Due(2), vm.state.value.review)
        }

    @Test
    fun `a failed check says so, and the old count never lingers`() =
        runTest {
            reviewsRepo.due = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Failed, vm.state.value.review)
            reviewsRepo.due = due(4)
            vm.retryReview()
            advanceUntilIdle()
            assertEquals(HomeReview.Due(4), vm.state.value.review)
            // Back from a session: the old count is dropped at once, never shown while the new one is on its way.
            reviewsRepo.due = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
            vm.onStart()
            assertEquals(HomeReview.Hidden, vm.state.value.review)
            advanceUntilIdle()
            assertEquals(HomeReview.Failed, vm.state.value.review)
        }

    @Test
    fun `a refused token is left to the folder list`() =
        runTest {
            reviewsRepo.due = ApiResult.Failure(ApiFailure.Unauthorized)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.onStart()
            advanceUntilIdle()
            assertEquals(HomeReview.Hidden, vm.state.value.review)
        }

    @Test
    fun `the permission is asked once, the first time the card shows due cards`() =
        runTest {
            reviewsRepo.due = due(3)
            val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            vm.effects.test {
                vm.onStart()
                advanceUntilIdle()
                assertEquals(HomeEffect.AskNotificationPermission, awaitItem())
                // Not launched yet: not used up, and not queued twice either.
                assertFalse(prefs.asked)
                vm.onStart()
                advanceUntilIdle()
                expectNoEvents()
                // The screen launched the dialog.
                vm.onNotificationPermissionRequested()
                advanceUntilIdle()
                assertTrue(prefs.asked)
                // Back on Home with the same due cards: not asked again.
                vm.onStart()
                advanceUntilIdle()
                expectNoEvents()
            }
        }

    @Test
    fun `an ask that was never launched is not used up`() =
        runTest {
            reviewsRepo.due = due(3)
            vm(FakeFolders(ApiResult.Success(listOf(folder("a"))))).apply {
                onStart()
                advanceUntilIdle()
            }
            // The screen went away before consuming the effect: a new view model asks again.
            assertFalse(prefs.asked)
            val next = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
            next.effects.test {
                next.onStart()
                advanceUntilIdle()
                assertEquals(HomeEffect.AskNotificationPermission, awaitItem())
            }
        }

    @Test
    fun `the permission is not asked without due cards, with the switch off, or when already granted`() =
        runTest {
            suspend fun asks(
                dueCount: Int,
                switchOn: Boolean = true,
                granted: Boolean = false,
            ): Boolean {
                prefs.on = switchOn
                prefs.asked = false
                permission.granted = granted
                reviewsRepo.due = due(dueCount)
                val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
                var asked = false
                vm.effects.test {
                    vm.onStart()
                    advanceUntilIdle()
                    asked =
                        cancelAndConsumeRemainingEvents().any {
                            it is app.cash.turbine.Event.Item &&
                                it.value == HomeEffect.AskNotificationPermission
                        }
                }
                return asked
            }
            assertFalse(asks(dueCount = 0))
            assertFalse(asks(dueCount = 3, switchOn = false))
            assertFalse(asks(dueCount = 3, granted = true))
            assertTrue(asks(dueCount = 3))
        }

    @Test
    fun `a grant from the dialog starts the hourly check`() =
        runTest {
            val vm = vm(FakeFolders())
            permission.granted = true
            vm.onNotificationPermissionResult()
            advanceUntilIdle()
            assertEquals(1, scheduler.schedules)
        }

    private class FakeResumeLessons(
        var result: ApiResult<com.teachme.core.model.LessonInProgress?> = ApiResult.Success(null),
    ) : com.teachme.core.model.LessonRepository {
        var calls = 0

        private fun unused(): Nothing = error("not used by Home")

        override suspend fun startGeneration(topicId: String) = unused()

        override suspend fun listLessons(
            topicId: String,
            cursor: String?,
        ) = unused()

        override suspend fun getLesson(lessonId: String) = unused()

        override suspend fun submitAttempts(
            lessonId: String,
            attempts: List<com.teachme.core.model.QueuedAttempt>,
        ) = unused()

        override suspend fun completeLesson(lessonId: String) = unused()

        override suspend fun putHighlights(
            lessonId: String,
            highlights: List<com.teachme.core.model.HighlightRecord>,
        ) = unused()

        override suspend fun putStrokes(
            lessonId: String,
            strokes: List<com.teachme.core.model.StrokeRecord>,
        ) = unused()

        override suspend fun listJobs(topicId: String) = unused()

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ) = unused()

        override suspend fun listGlossary(topicId: String) = unused()

        override suspend fun listRecords(topicId: String) = unused()

        override suspend fun lastInProgress(): ApiResult<com.teachme.core.model.LessonInProgress?> {
            calls++
            return result
        }
    }

    private val resumeLesson =
        com.teachme.core.model.LessonInProgress(
            com.teachme.core.model.Lesson(
                "l1",
                "t1",
                2,
                "Titolo",
                "",
                5,
                com.teachme.core.model.LessonStatus.InProgress,
                1,
                1,
                null,
                null,
            ),
            "Argomento",
            Instant.EPOCH,
        )

    @Test
    fun `resume card shows the lesson in progress and reloads on every start`() =
        runTest {
            lessonsRepo.result = ApiResult.Success(resumeLesson)
            val model = vm(FakeFolders())
            model.onStart()
            advanceUntilIdle()
            assertEquals(resumeLesson, model.state.value.resume)
            lessonsRepo.result = ApiResult.Success(null)
            model.onStart()
            advanceUntilIdle()
            assertNull(model.state.value.resume)
            assertEquals(2, lessonsRepo.calls)
        }

    @Test
    fun `resume failure hides the card without any banner`() =
        runTest {
            lessonsRepo.result = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
            val model = vm(FakeFolders())
            model.onStart()
            advanceUntilIdle()
            assertNull(model.state.value.resume)
            assertFalse(model.state.value.refreshFailed)
        }

    @Test
    fun `resume failure after a success keeps the card`() =
        runTest {
            lessonsRepo.result = ApiResult.Success(resumeLesson)
            val model = vm(FakeFolders())
            model.onStart()
            advanceUntilIdle()
            lessonsRepo.result = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
            model.onStart()
            advanceUntilIdle()
            assertEquals(resumeLesson, model.state.value.resume)
        }

    // --- Filtro per argomento ----------------------------------------------------------------------

    private val rust = DueTopic("t-rust", "Rust", 8)
    private val sql = DueTopic("t-sql", "SQL", 4)

    private fun dueTopics(vararg topics: DueTopic) =
        ApiResult.Success(DueReviews(emptyList(), topics.sumOf { it.dueCount }, 0, null, topics.toList()))

    private suspend fun kotlinx.coroutines.test.TestScope.started(): HomeViewModel {
        val vm = vm(FakeFolders(ApiResult.Success(listOf(folder("a")))))
        vm.onStart()
        advanceUntilIdle()
        return vm
    }

    @Test
    fun `the review card carries the topics for the chips`() =
        runTest {
            reviewsRepo.due = dueTopics(rust, sql)
            val vm = started()
            assertEquals(listOf(rust, sql), (vm.state.value.review as HomeReview.Due).topics)
            assertNull(vm.state.value.reviewTopicId)
        }

    @Test
    fun `a single topic is carried but the chips need at least two`() =
        runTest {
            reviewsRepo.due = dueTopics(rust)
            val vm = started()
            assertEquals(1, (vm.state.value.review as HomeReview.Due).topics.size)
        }

    @Test
    fun `picking a chip selects it without asking again, Tutti clears it`() =
        runTest {
            reviewsRepo.due = dueTopics(rust, sql)
            val vm = started()
            vm.selectReviewTopic("t-sql")
            assertEquals("t-sql", vm.state.value.reviewTopicId)
            vm.selectReviewTopic(null)
            assertNull(vm.state.value.reviewTopicId)
            vm.selectReviewTopic("unknown")
            assertNull(vm.state.value.reviewTopicId)
        }

    @Test
    fun `the selected topic going away goes back to Tutti`() =
        runTest {
            reviewsRepo.due = dueTopics(rust, sql)
            val vm = started()
            vm.selectReviewTopic("t-sql")
            reviewsRepo.due = dueTopics(rust)
            vm.retryReview()
            advanceUntilIdle()
            assertNull(vm.state.value.reviewTopicId)
        }

    @Test
    fun `back in the foreground the filter is Tutti again`() =
        runTest {
            reviewsRepo.due = dueTopics(rust, sql)
            val vm = started()
            vm.selectReviewTopic("t-sql")
            vm.onStart()
            advanceUntilIdle()
            assertNull(vm.state.value.reviewTopicId)
        }

    @Test
    fun `a ViewModel built after another one was cleared starts without its selection`() =
        runTest {
            val libraryA = FakeLibraryState()
            val a = vm(FakeFolders(ApiResult.Success(listOf(folder("a1", "Segreto di A"), folder("a2")))), libraryA)
            a.onStart()
            advanceUntilIdle()
            a.select("a2")
            advanceUntilIdle()
            assertEquals("a2", a.state.value.selectedFolderId)

            // Sign-out clears A's navigation entry; B signs in on the same device with its own handle and data.
            a.viewModelScope.cancel()
            val b = vm(FakeFolders(ApiResult.Success(listOf(folder("b1", "Cartella di B")))), FakeLibraryState())
            assertNull(b.state.value.selectedFolderId)
            assertFalse(b.state.value.selectionByUser)
            b.onStart()
            advanceUntilIdle()

            assertEquals(
                listOf("b1"),
                b.state.value.folders
                    ?.map { it.id },
            )
            assertEquals("b1", b.state.value.selectedFolderId)
            assertFalse(b.state.value.selectionByUser)
        }
}
