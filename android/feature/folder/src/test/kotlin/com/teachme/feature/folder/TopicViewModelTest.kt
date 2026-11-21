package com.teachme.feature.folder

import app.cash.turbine.test
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.DueReviews
import com.teachme.core.model.FieldIssue
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashKind
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LoadError
import com.teachme.core.model.ManualUserScopes
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewQueue
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicRepository
import com.teachme.core.model.TopicState
import com.teachme.core.model.TopicTabNames
import com.teachme.core.model.UnreachableReason
import com.teachme.feature.lesson.TopicReviewEntry
import com.teachme.feature.mission.TopicTab
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class TopicViewModelTest {
    private fun topic(
        title: String = "Rust",
        state: TopicState = TopicState.New,
    ) = Topic("t1", "f1", title, state, Instant.EPOCH, Instant.EPOCH)

    private class FakeTopics(
        var get: ApiResult<Topic>,
    ) : TopicRepository {
        var update: (String) -> ApiResult<Topic> = { error("unused") }
        var delete: ApiResult<Unit> = ApiResult.Success(Unit)

        override suspend fun listTopics(folderId: String) = error("unused")

        var gate: CompletableDeferred<Unit>? = null

        override suspend fun getTopic(id: String): ApiResult<Topic> {
            val snapshot = get
            gate?.await()
            return snapshot
        }

        override suspend fun createTopic(
            folderId: String,
            title: String,
            language: TopicLanguage,
        ) = error("unused")

        override suspend fun updateTopic(
            id: String,
            title: String?,
            folderId: String?,
        ) = update(title.orEmpty())

        override suspend fun deleteTopic(id: String) = delete
    }

    private class FakeReviews : ReviewRepository {
        var due: ApiResult<DueReviews> = ApiResult.Success(DueReviews(emptyList(), 0, 0, null))
        val calls = mutableListOf<Pair<Int, String?>>()

        override suspend fun due(
            limit: Int,
            topicId: String?,
        ): ApiResult<DueReviews> {
            calls += limit to topicId
            return due
        }

        var rate: ApiResult<ReviewCard> = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Other))
        val rated = mutableListOf<QueuedReview>()

        override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> {
            rated += review
            return rate
        }

        override suspend fun undo(cardId: String) = error("unused")

        override suspend fun suspend(cardId: String) = error("unused")

        override suspend fun unsuspend(cardId: String) = error("unused")

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ) = error("unused")
    }

    private val reviewsRepo = FakeReviews()

    private class MemoryQueue : ReviewQueue {
        val items = mutableListOf<QueuedReview>()

        override suspend fun enqueue(review: QueuedReview) {
            items += review
        }

        override suspend fun pending() = items.toList()

        override suspend fun remove(review: QueuedReview) = items.removeIf { it.sameReview(review) }

        override fun count(): Flow<Int> = flowOf(items.size)
    }

    private val queue = MemoryQueue()
    private val syncer = ReviewSyncer(reviewsRepo, queue, ManualUserScopes())

    private fun dueResult(count: Int) = ApiResult.Success(DueReviews(emptyList(), count, count, null))

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `load shows the state chip data`() =
        runTest {
            val vm =
                TopicViewModel(
                    "t1",
                    "Rust",
                    FakeTopics(ApiResult.Success(topic(state = TopicState.Ready))),
                    InMemoryTopicTabInbox(),
                    reviewsRepo,
                    syncer,
                )
            assertNull(vm.state.value.topic)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(
                TopicState.Ready,
                vm.state.value.topic
                    ?.state,
            )
        }

    @Test
    fun `load failure and not found`() =
        runTest {
            val repo = FakeTopics(ApiResult.Failure(ApiFailure.Unauthorized))
            val vm = TopicViewModel("t1", "Rust", repo, InMemoryTopicTabInbox(), reviewsRepo, syncer)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(LoadError.Unauthorized, vm.state.value.loadError)

            repo.get = ApiResult.Failure(ApiFailure.NotFound)
            vm.effects.test {
                vm.retry()
                advanceUntilIdle()
                assertEquals(TopicEffect.Leave(Flash(FlashKind.TopicGone)), awaitItem())
            }
        }

    @Test
    fun `rename success, duplicate and no-op`() =
        runTest {
            val repo = FakeTopics(ApiResult.Success(topic()))
            val vm = TopicViewModel("t1", "Rust", repo, InMemoryTopicTabInbox(), reviewsRepo, syncer)
            vm.onStart()
            advanceUntilIdle()

            repo.update = { ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("title", "duplicate")))) }
            vm.openRename()
            vm.rename("Altro")
            advanceUntilIdle()
            assertEquals(
                FieldIssue.Duplicate,
                vm.state.value.dialog
                    ?.ui
                    ?.error
                    ?.field,
            )

            repo.update = { ApiResult.Success(topic(it)) }
            vm.effects.test {
                vm.rename("Rust avanzato")
                advanceUntilIdle()
                assertEquals(TopicEffect.Renamed("Rust avanzato"), awaitItem())
                assertEquals(TopicEffect.Changed, awaitItem())
            }
            assertEquals("Rust avanzato", vm.state.value.title)
            assertNull(vm.state.value.dialog)
        }

    @Test
    fun `delete leaves with the title for the toast`() =
        runTest {
            val vm =
                TopicViewModel(
                    "t1",
                    "Rust",
                    FakeTopics(ApiResult.Success(topic())),
                    InMemoryTopicTabInbox(),
                    reviewsRepo,
                    syncer,
                )
            vm.onStart()
            advanceUntilIdle()
            vm.openDelete()
            vm.effects.test {
                vm.confirmDelete()
                advanceUntilIdle()
                assertEquals(TopicEffect.Leave(Flash(FlashKind.TopicDeleted, "Rust")), awaitItem())
            }
        }

    @Test
    fun `a reload started before a rename does not bring the old title back`() =
        runTest {
            val repo = FakeTopics(ApiResult.Success(topic()))
            val vm = TopicViewModel("t1", "Rust", repo, InMemoryTopicTabInbox(), reviewsRepo, syncer)
            vm.onStart()
            advanceUntilIdle()

            val gate = CompletableDeferred<Unit>()
            repo.gate = gate
            vm.retry()
            advanceUntilIdle()
            repo.update = { ApiResult.Success(topic(it)) }
            vm.openRename()
            vm.rename("Nuovo titolo")
            advanceUntilIdle()
            gate.complete(Unit)
            advanceUntilIdle()

            assertEquals("Nuovo titolo", vm.state.value.title)
            assertEquals(
                "Nuovo titolo",
                vm.state.value.topic
                    ?.title,
            )
        }

    @Test
    fun `the first tab depends on the state and is remembered for the session`() =
        runTest {
            val chatFirst =
                TopicViewModel(
                    "t1",
                    "Rust",
                    FakeTopics(ApiResult.Success(topic(state = TopicState.MissionProposed))),
                    InMemoryTopicTabInbox(),
                    reviewsRepo,
                    syncer,
                )
            chatFirst.onStart()
            advanceUntilIdle()
            assertEquals(TopicTab.Chat, chatFirst.state.value.tab)

            val researching = FakeTopics(ApiResult.Success(topic(state = TopicState.Researching)))
            val vm = TopicViewModel("t1", "Rust", researching, InMemoryTopicTabInbox(), reviewsRepo, syncer)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicTab.Resources, vm.state.value.tab)

            // A later reload or a Topic update on the stream must not undo the user's choice.
            vm.selectTab(TopicTab.Chat)
            vm.onStart()
            advanceUntilIdle()
            vm.onTopicFromChat(topic(state = TopicState.Ready))
            assertEquals(TopicTab.Chat, vm.state.value.tab)
        }

    @Test
    fun `a state change on the stream does not move the tab by itself`() =
        runTest {
            val vm =
                TopicViewModel(
                    "t1",
                    "Rust",
                    FakeTopics(ApiResult.Success(topic(state = TopicState.Interviewing))),
                    InMemoryTopicTabInbox(),
                    reviewsRepo,
                    syncer,
                )
            vm.onStart()
            advanceUntilIdle()
            vm.onTopicFromChat(topic(state = TopicState.Researching))
            assertEquals(TopicTab.Chat, vm.state.value.tab)
        }

    @Test
    fun `a ready topic opens on the lessons and another screen can ask for a tab`() =
        runTest {
            val inbox = InMemoryTopicTabInbox()
            val vm =
                TopicViewModel(
                    "t1",
                    "Rust",
                    FakeTopics(ApiResult.Success(topic(state = TopicState.Ready))),
                    inbox,
                    reviewsRepo,
                    syncer,
                )
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicTab.Lessons, vm.state.value.tab)

            // "Vedi tutto nel registro" from the summary: the request is consumed once, on the next start.
            vm.selectTab(TopicTab.Chat)
            inbox.post("t1", TopicTabNames.PROGRESS)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicTab.Progress, vm.state.value.tab)
            vm.selectTab(TopicTab.Mission)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicTab.Mission, vm.state.value.tab)
        }

    private fun readyVm() =
        TopicViewModel(
            "t1",
            "Rust",
            FakeTopics(ApiResult.Success(topic(state = TopicState.Ready))),
            InMemoryTopicTabInbox(),
            reviewsRepo,
            syncer,
        )

    @Test
    fun `review row is absent with nothing due`() =
        runTest {
            val vm = readyVm()
            vm.onStart()
            advanceUntilIdle()
            assertNull(vm.state.value.review)
            assertEquals(listOf(1 to "t1"), reviewsRepo.calls)
        }

    @Test
    fun `review row shows the cards and minutes, capped to one session`() =
        runTest {
            reviewsRepo.due = dueResult(8)
            val vm = readyVm()
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicReviewEntry(8, 2), vm.state.value.review)
            reviewsRepo.due = dueResult(34)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(TopicReviewEntry(20, 5), vm.state.value.review)
        }

    @Test
    fun `review row goes away on error and when nothing is due any more`() =
        runTest {
            reviewsRepo.due = dueResult(3)
            val vm = readyVm()
            vm.onStart()
            advanceUntilIdle()
            reviewsRepo.due = ApiResult.Failure(ApiFailure.Unauthorized)
            vm.onStart()
            advanceUntilIdle()
            assertNull(vm.state.value.review)
        }

    @Test
    fun `review row sends the queued grades first and counts net of those still queued`() =
        runTest {
            queue.enqueue(QueuedReview("c1", "good", 0, 0))
            queue.enqueue(QueuedReview("c2", "good", 0, 0))
            reviewsRepo.due = dueResult(3)
            val vm = readyVm()
            vm.onStart()
            advanceUntilIdle()
            // The flush was tried before the count (it stops at the first offline failure); both stay queued: 3 due
            // on the backend, 1 really left.
            assertEquals(listOf("c1"), reviewsRepo.rated.map { it.cardId })
            assertEquals(TopicReviewEntry(1, 1), vm.state.value.review)

            queue.enqueue(QueuedReview("c3", "good", 0, 0))
            vm.onStart()
            advanceUntilIdle()
            assertNull(vm.state.value.review)
        }
}
