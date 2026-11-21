package com.teachme.feature.folder

import app.cash.turbine.test
import com.teachme.core.model.ActionFailure
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.FieldIssue
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashKind
import com.teachme.core.model.Folder
import com.teachme.core.model.FolderRepository
import com.teachme.core.model.InMemoryFlashInbox
import com.teachme.core.model.LoadError
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicRepository
import com.teachme.core.model.TopicState
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class FolderViewModelTest {
    private fun topic(
        id: String,
        title: String = id,
        state: TopicState = TopicState.New,
    ) = Topic(id, "f1", title, state, Instant.EPOCH, Instant.EPOCH)

    private fun folder(count: Int = 0) = Folder("f1", "Lavoro", 0, count, Instant.EPOCH, Instant.EPOCH)

    private class FakeFolders(
        var get: ApiResult<Folder>,
    ) : FolderRepository {
        var rename: (
            String,
        ) -> ApiResult<Folder> = { ApiResult.Success(Folder("f1", it, 0, 0, Instant.EPOCH, Instant.EPOCH)) }
        var delete: ApiResult<Unit> = ApiResult.Success(Unit)

        override suspend fun listFolders() = error("unused")

        override suspend fun getFolder(id: String) = get

        override suspend fun createFolder(name: String) = error("unused")

        override suspend fun renameFolder(
            id: String,
            name: String,
        ) = rename(name)

        override suspend fun deleteFolder(id: String) = delete
    }

    private class FakeTopics(
        var list: ApiResult<List<Topic>>,
    ) : TopicRepository {
        /** While set, list calls answer with what they saw at call time, but only once it completes. */
        var gate: CompletableDeferred<Unit>? = null

        var create: (
            String,
        ) -> ApiResult<Topic> = {
            ApiResult.Success(
                Topic("new", "f1", it, TopicState.New, Instant.EPOCH, Instant.EPOCH),
            )
        }
        var update: (
            String,
        ) -> ApiResult<Topic> = {
            ApiResult.Success(
                Topic("t1", "f1", it, TopicState.New, Instant.EPOCH, Instant.EPOCH),
            )
        }
        var delete: ApiResult<Unit> = ApiResult.Success(Unit)
        val createdLanguages = mutableListOf<TopicLanguage>()

        override suspend fun listTopics(folderId: String): ApiResult<List<Topic>> {
            val snapshot = list
            gate?.await()
            return snapshot
        }

        override suspend fun getTopic(id: String) = error("unused")

        override suspend fun createTopic(
            folderId: String,
            title: String,
            language: TopicLanguage,
        ): ApiResult<Topic> {
            createdLanguages += language
            return create(title)
        }

        override suspend fun updateTopic(
            id: String,
            title: String?,
            folderId: String?,
        ) = update(title.orEmpty())

        override suspend fun deleteTopic(id: String) = delete
    }

    private val inbox = InMemoryFlashInbox()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun vm(
        folders: FakeFolders = FakeFolders(ApiResult.Success(folder(2))),
        topics: FakeTopics = FakeTopics(ApiResult.Success(listOf(topic("t1", "Rust"), topic("t2", "Foto")))),
    ) = FolderViewModel("f1", "Lavoro", folders, topics, inbox)

    @Test
    fun `load shows the topics and the authoritative count`() =
        runTest {
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()

            assertEquals(
                listOf("t1", "t2"),
                vm.state.value.topics
                    ?.map { it.id },
            )
            assertEquals(2, vm.state.value.topicCount)
            assertEquals("Lavoro", vm.state.value.folderName)
        }

    @Test
    fun `folder not found leaves with a flash`() =
        runTest {
            val vm = vm(folders = FakeFolders(ApiResult.Failure(ApiFailure.NotFound)))
            vm.effects.test {
                vm.onStart()
                advanceUntilIdle()
                assertEquals(FolderEffect.Leave(Flash(FlashKind.FolderGone)), awaitItem())
            }
        }

    @Test
    fun `first load error and refresh banner`() =
        runTest {
            val topics = FakeTopics(ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)))
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(LoadError.Network, vm.state.value.loadError)

            topics.list = ApiResult.Success(listOf(topic("t1")))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.loadError)

            topics.list = ApiResult.Failure(ApiFailure.Unexpected(500))
            vm.onStart()
            advanceUntilIdle()
            assertTrue(vm.state.value.refreshFailed)
            assertEquals(
                1,
                vm.state.value.topics
                    ?.size,
            )
        }

    @Test
    fun `create topic appends it, bumps the count and offers to open it`() =
        runTest {
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()
            vm.openCreateTopic()

            vm.effects.test {
                vm.createTopic(" Chitarra ")
                advanceUntilIdle()
                val toast = awaitItem() as FolderEffect.Toast
                assertEquals("Chitarra", (toast.message as FolderMessage.TopicCreated).topic.title)
                assertEquals(FolderEffect.ScrollTo("new"), awaitItem())
                assertEquals(FolderEffect.Changed, awaitItem())
            }
            assertEquals(3, vm.state.value.topicCount)
            assertEquals(
                "new",
                vm.state.value.topics
                    ?.last()
                    ?.id,
            )
            assertNull(vm.state.value.dialog)
        }

    @Test
    fun `create dialog preselects English only for an English device and sends the chosen language`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(emptyList()))
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreateTopic(deviceLanguage = "fr")
            assertEquals(TopicLanguage.It, (vm.state.value.dialog as FolderDialog.CreateTopic).language)
            vm.openCreateTopic(deviceLanguage = "en")
            assertEquals(TopicLanguage.En, (vm.state.value.dialog as FolderDialog.CreateTopic).language)
            vm.selectCreateLanguage(TopicLanguage.It)
            vm.createTopic("Rust")
            advanceUntilIdle()
            assertEquals(listOf(TopicLanguage.It), topics.createdLanguages)
        }

    @Test
    fun `duplicate title is a field error on the open dialog`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(emptyList()))
            topics.create = { ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("title", "duplicate")))) }
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreateTopic()
            vm.createTopic("Rust")
            advanceUntilIdle()

            assertEquals(
                FieldIssue.Duplicate,
                vm.state.value.dialog
                    ?.ui
                    ?.error
                    ?.field,
            )
        }

    @Test
    fun `create in a folder deleted elsewhere leaves`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(emptyList()))
            topics.create = { ApiResult.Failure(ApiFailure.NotFound) }
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()
            vm.openCreateTopic()

            vm.effects.test {
                vm.createTopic("Rust")
                advanceUntilIdle()
                assertEquals(FolderEffect.Leave(Flash(FlashKind.FolderGone)), awaitItem())
            }
        }

    @Test
    fun `rename topic updates the row, unchanged title makes no call`() =
        runTest {
            var calls = 0
            val topics = FakeTopics(ApiResult.Success(listOf(topic("t1", "Rust"))))
            topics.update = {
                calls++
                ApiResult.Success(topic("t1", it))
            }
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()

            vm.openRenameTopic(topic("t1", "Rust"))
            vm.renameTopic("Rust")
            advanceUntilIdle()
            assertEquals(0, calls)

            vm.openRenameTopic(topic("t1", "Rust"))
            vm.renameTopic("Rust avanzato")
            advanceUntilIdle()
            assertEquals(
                "Rust avanzato",
                vm.state.value.topics
                    ?.single()
                    ?.title,
            )
        }

    @Test
    fun `delete topic network failure keeps the dialog, a retry succeeds`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(listOf(topic("t1", "Rust"), topic("t2"))))
            topics.delete = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Other))
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()
            vm.openDeleteTopic(topic("t1", "Rust"))
            vm.confirmDeleteTopic()
            advanceUntilIdle()
            assertEquals(
                ActionFailure.Network,
                vm.state.value.dialog
                    ?.ui
                    ?.error
                    ?.action,
            )
            assertEquals(
                2,
                vm.state.value.topics
                    ?.size,
            )

            topics.delete = ApiResult.Success(Unit)
            vm.confirmDeleteTopic()
            advanceUntilIdle()
            assertNull(vm.state.value.dialog)
            assertEquals(
                listOf("t2"),
                vm.state.value.topics
                    ?.map { it.id },
            )
            assertEquals(1, vm.state.value.topicCount)
        }

    @Test
    fun `rename folder updates the title, delete folder leaves with its name`() =
        runTest {
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()

            vm.openRenameFolder()
            vm.renameFolder("Studio")
            advanceUntilIdle()
            assertEquals("Studio", vm.state.value.folderName)

            vm.openDeleteFolder()
            vm.effects.test {
                assertEquals(FolderEffect.Toast(FolderMessage.FolderRenamed("Studio")), awaitItem())
                assertEquals(FolderEffect.Changed, awaitItem())
                vm.confirmDeleteFolder()
                advanceUntilIdle()
                assertEquals(FolderEffect.Leave(Flash(FlashKind.FolderDeleted, "Studio")), awaitItem())
            }
        }

    @Test
    fun `a flash from the topic screen becomes a toast`() =
        runTest {
            val vm = vm()
            inbox.post(Flash(FlashKind.TopicDeleted, "Rust"))
            vm.effects.test {
                vm.onStart()
                advanceUntilIdle()
                assertEquals(FolderEffect.Toast(FolderMessage.TopicDeleted("Rust")), awaitItem())
            }
        }

    @Test
    fun `a refresh started before a create does not wipe the new topic`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(listOf(topic("t1"))))
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()

            val gate = CompletableDeferred<Unit>()
            topics.gate = gate
            vm.retry()
            advanceUntilIdle()
            vm.openCreateTopic()
            vm.createTopic("Nuovo")
            advanceUntilIdle()
            gate.complete(Unit)
            advanceUntilIdle()

            assertEquals(
                listOf("t1", "new"),
                vm.state.value.topics
                    ?.map { it.id },
            )
        }

    @Test
    fun `a refresh started before a delete does not bring the topic back`() =
        runTest {
            val topics = FakeTopics(ApiResult.Success(listOf(topic("t1"), topic("t2"))))
            val vm = vm(topics = topics)
            vm.onStart()
            advanceUntilIdle()

            val gate = CompletableDeferred<Unit>()
            topics.gate = gate
            vm.retry()
            advanceUntilIdle()
            vm.openDeleteTopic(topic("t1"))
            vm.confirmDeleteTopic()
            advanceUntilIdle()
            gate.complete(Unit)
            advanceUntilIdle()

            assertEquals(
                listOf("t2"),
                vm.state.value.topics
                    ?.map { it.id },
            )
        }

    @Test
    fun `deleting a topic that was already gone does not decrement the count and reloads`() =
        runTest {
            val folders = FakeFolders(ApiResult.Success(folder(2)))
            val topics = FakeTopics(ApiResult.Success(listOf(topic("t1"), topic("t2"))))
            topics.delete = ApiResult.Failure(ApiFailure.NotFound)
            val vm = vm(folders, topics)
            vm.onStart()
            advanceUntilIdle()

            // Another client already deleted t1 and the folder now reports 1.
            folders.get = ApiResult.Success(folder(1))
            topics.list = ApiResult.Success(listOf(topic("t2")))
            vm.openDeleteTopic(topic("t1"))
            vm.confirmDeleteTopic()
            advanceUntilIdle()

            assertEquals(1, vm.state.value.topicCount)
            assertEquals(
                listOf("t2"),
                vm.state.value.topics
                    ?.map { it.id },
            )
        }
}
