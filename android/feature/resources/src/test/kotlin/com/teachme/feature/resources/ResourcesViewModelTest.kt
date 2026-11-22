package com.teachme.feature.resources

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.ResourceRepository
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.TopicJob
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
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

/** Remembers the credit like the real monitor: `noteObserved(Credit)` sets the flag, `noteWorked` clears it. */
private class CreditTrackingMonitor : LlmKeyMonitor by NoLlmKeyMonitor {
    private val flow = kotlinx.coroutines.flow.MutableStateFlow(LlmKeyStatus())
    override val status: kotlinx.coroutines.flow.StateFlow<LlmKeyStatus> get() = flow
    val observed = mutableListOf<LlmKeyVariant>()

    // Delegation (`by`) does not use the interface default: forward it.
    override fun noteFailedNow(
        variant: LlmKeyVariant,
        serverDate: java.time.Instant?,
    ) = noteObserved(variant, serverDate ?: java.time.Instant.now())

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: java.time.Instant,
    ) {
        observed += variant
        if (variant == LlmKeyVariant.Credit) flow.value = flow.value.copy(creditExhaustedSeen = true)
    }

    override fun noteWorked(at: java.time.Instant) {
        flow.value = flow.value.copy(creditExhaustedSeen = false)
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class ResourcesViewModelTest {
    private class FakeResources : ResourceRepository {
        var view: ApiResult<ResourcesView> =
            ApiResult.Success(
                ResourcesView(emptyList(), emptyList(), false, null, null),
            )
        var start: ApiResult<TopicJob> =
            ApiResult.Success(
                job(JobStatus.Queued, id = "j2", created = Instant.parse("2026-11-09T15:00:00Z")),
            )
        var retry: ApiResult<TopicJob> = ApiResult.Success(job(JobStatus.Queued))
        var getCalls = 0
        var onGet: (() -> Unit)? = null
        var startCalls = 0
        val retried = mutableListOf<String>()
        val views = ArrayDeque<ApiResult<ResourcesView>>()

        override suspend fun getResources(topicId: String): ApiResult<ResourcesView> {
            getCalls++
            onGet?.invoke()
            return views.removeFirstOrNull() ?: view
        }

        override suspend fun startResearch(topicId: String): ApiResult<TopicJob> {
            startCalls++
            return start
        }

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ): ApiResult<TopicJob> {
            retried += jobId
            return retry
        }
    }

    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val network = ApiFailure.Network(UnreachableReason.Timeout)

    private fun viewModel(repo: FakeResources): ResourcesViewModel {
        val vm = ResourcesViewModel("t1", repo, CreditTrackingMonitor())
        vm.clock = { dispatcherTime(vm) }
        return vm
    }

    // The coalescing clock follows the virtual time of the test scheduler.
    private fun dispatcherTime(
        @Suppress("UNUSED_PARAMETER") vm: ResourcesViewModel,
    ) = dispatcher.scheduler.currentTime

    private fun TestScope.started(
        repo: FakeResources,
        initial: ResourcesView? = null,
    ): ResourcesViewModel {
        if (initial != null) repo.view = ApiResult.Success(initial)
        val vm = viewModel(repo)
        vm.onEvent(ChatEvent.ResyncRequired)
        advanceUntilIdle()
        return vm
    }

    private fun running(step: String? = "searching") = job(JobStatus.Running, step = step)

    @Test
    fun `loads the view and a failed first load shows the error`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(listOf(source("a"))))
            assertEquals(ResourcesLoad.Ready, vm.state.value.load)
            assertEquals(
                1,
                vm.state.value.view
                    ?.resources
                    ?.size,
            )

            val failing = FakeResources().apply { view = ApiResult.Failure(network) }
            val vm2 = started(failing)
            assertEquals(ResourcesLoad.Failed(LoadError.Network), vm2.state.value.load)
            failing.view = ApiResult.Success(view())
            vm2.retryLoad()
            advanceUntilIdle()
            assertEquals(ResourcesLoad.Ready, vm2.state.value.load)
        }

    @Test
    fun `a failed refresh keeps what is on screen`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(listOf(source("a"))))
            repo.view = ApiResult.Failure(network)
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertEquals(ResourcesLoad.Ready, vm.state.value.load)
            assertEquals(
                1,
                vm.state.value.view
                    ?.resources
                    ?.size,
            )
        }

    @Test
    fun `job_updated upserts the research job and ignores other kinds and topics`() =
        runTest(dispatcher) {
            val vm = started(FakeResources(), view(job = job(JobStatus.Queued)))
            vm.onEvent(ChatEvent.JobUpdated(running("planning")))
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
            assertEquals(
                "planning",
                vm.state.value.view
                    ?.job
                    ?.progress
                    ?.step,
            )

            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Failed).copy(kind = "lesson")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Failed).copy(topicId = "other")))
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `a replayed older job never replaces the current one`() =
        runTest(dispatcher) {
            val newer = job(JobStatus.Running, id = "j2", created = Instant.parse("2026-11-09T16:00:00Z"))
            val vm = started(FakeResources(), view(job = newer))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, id = "j1")))
            assertEquals(
                "j2",
                vm.state.value.view
                    ?.job
                    ?.id,
            )
        }

    @Test
    fun `a job that arrives before the first load is carried by the fetch`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = viewModel(repo)
            vm.onEvent(ChatEvent.ResyncRequired)
            // The event lands before the view is there: nothing is applied, the fetch carries the job.
            vm.onEvent(ChatEvent.JobUpdated(running()))
            repo.view = ApiResult.Success(view(job = running()))
            advanceUntilIdle()
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `progress-only updates are coalesced to one per second`() =
        runTest(dispatcher) {
            val vm = started(FakeResources(), view(job = running("searching")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "searching", message = "Primo")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "searching", message = "Secondo")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "searching", message = "Terzo")))
            // Applied at most every second: the first change went through, the others wait.
            assertEquals(
                "Primo",
                vm.state.value.view
                    ?.job
                    ?.progress
                    ?.message,
            )
            advanceTimeBy(PROGRESS_COALESCE_MS + 1)
            assertEquals(
                "Terzo",
                vm.state.value.view
                    ?.job
                    ?.progress
                    ?.message,
            )
            // A change of phase is never delayed.
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "reading")))
            assertEquals(
                "reading",
                vm.state.value.view
                    ?.job
                    ?.progress
                    ?.step,
            )
        }

    @Test
    fun `success reloads the list, marks new sources and emits one effect`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val old =
                view(listOf(source("a")), job(JobStatus.Succeeded, id = "old"), ResearchInfo("old", 1, Instant.EPOCH))
            val vm = started(repo, old.copy(job = running()))
            val fresh =
                view(
                    listOf(source("a"), source("b")),
                    job(JobStatus.Succeeded, resourceCount = 2),
                    ResearchInfo("j1", 2, Instant.EPOCH),
                )
            repo.view = ApiResult.Success(fresh)
            val effects = mutableListOf<ResourcesEffect>()
            val collector = launchCollect(vm, effects)

            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, resourceCount = 2)))
            assertFalse(vm.state.value.settled)
            advanceUntilIdle()

            assertEquals(
                2,
                vm.state.value.view
                    ?.resources
                    ?.size,
            )
            assertTrue(vm.state.value.settled)
            assertEquals(setOf("https://example.org/b"), vm.state.value.newUrls)
            assertEquals(listOf<ResourcesEffect>(ResourcesEffect.Succeeded(2, refresh = true)), effects)
            assertEquals(
                2,
                vm.state.value.announcement
                    ?.count,
            )

            // The same event replayed on a reconnect changes nothing and does not toast again.
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, resourceCount = 2)))
            advanceUntilIdle()
            assertEquals(1, effects.size)

            vm.markSeen()
            assertTrue(
                vm.state.value.newUrls
                    .isEmpty(),
            )
            collector.cancel()
        }

    @Test
    fun `the first search ends without the refresh flag and without marks`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = running()))
            repo.view =
                ApiResult.Success(
                    view(
                        listOf(source("a")),
                        job(JobStatus.Succeeded, resourceCount = 1),
                        ResearchInfo("j1", 1, Instant.EPOCH),
                    ),
                )
            val effects = mutableListOf<ResourcesEffect>()
            val collector = launchCollect(vm, effects)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, resourceCount = 1)))
            advanceUntilIdle()
            assertEquals(listOf<ResourcesEffect>(ResourcesEffect.Succeeded(1, refresh = false)), effects)
            assertTrue(
                vm.state.value.newUrls
                    .isEmpty(),
            )
            collector.cancel()
        }

    @Test
    fun `if the new list cannot be fetched the old one is shown instead of a panel that never ends`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(listOf(source("a")), running(), ResearchInfo("old", 1, Instant.EPOCH)))
            repo.view = ApiResult.Failure(network)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded)))
            advanceUntilIdle()
            assertTrue(vm.state.value.settled)
            assertEquals(
                JobStatus.Succeeded,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `a job that fails while running emits the failure effect once`() =
        runTest(dispatcher) {
            val vm = started(FakeResources(), view(job = running()))
            val effects = mutableListOf<ResourcesEffect>()
            val collector = launchCollect(vm, effects)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Failed, errorCode = "timeout")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Failed, errorCode = "timeout")))
            advanceUntilIdle()
            assertEquals(listOf<ResourcesEffect>(ResourcesEffect.Failed), effects)
            collector.cancel()
        }

    @Test
    fun `retry requeues the same job and shows it at once`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed, errorCode = "upstream_failed")))
            vm.retry()
            advanceUntilIdle()
            assertEquals(listOf("j1"), repo.retried)
            assertEquals(
                JobStatus.Queued,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
            assertFalse(vm.state.value.acting)
        }

    @Test
    fun `retry does nothing for a job that is not failed`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = running()))
            vm.retry()
            advanceUntilIdle()
            assertTrue(repo.retried.isEmpty())
        }

    @Test
    fun `409 on retry reloads in silence and shows the truth`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed)))
            repo.retry = ApiResult.Failure(ApiFailure.Conflict)
            repo.view = ApiResult.Success(view(job = running("reading")))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.actionError)
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
            assertFalse(vm.state.value.acting)
        }

    @Test
    fun `network error on retry is shown and the user can try again`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed)))
            repo.retry = ApiResult.Failure(network)
            vm.retry()
            advanceUntilIdle()
            assertEquals(LoadError.Network, vm.state.value.actionError)
            assertFalse(vm.state.value.acting)
            repo.retry = ApiResult.Success(job(JobStatus.Queued))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.actionError)
            assertEquals(
                JobStatus.Queued,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `a research or a retry refused for the key shows a key notice, not a network error`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed)))

            repo.retry = ApiResult.Failure(ApiFailure.LlmKeyRejected)
            vm.retry()
            advanceUntilIdle()
            val first = vm.state.value
            assertEquals(LlmKeyVariant.Rejected, first.keyNotice?.variant)
            assertNull(first.actionError)
            assertFalse(first.acting)

            // The next attempt clears it, and a new refusal is a new notice.
            repo.retry = ApiResult.Failure(ApiFailure.LlmKeyMissing)
            vm.retry()
            advanceUntilIdle()
            assertEquals(
                LlmKeyVariant.Missing,
                vm.state.value.keyNotice
                    ?.variant,
            )
            assertTrue(
                vm.state.value.keyNotice!!
                    .id > first.keyNotice!!.id,
            )

            repo.retry = ApiResult.Success(job(JobStatus.Queued))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.keyNotice)
        }

    @Test
    fun `a retry or a new research refused for the daily budget carries the reset, not an error`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed)))
            val before =
                java.time.Instant
                    .now()
                    .truncatedTo(java.time.temporal.ChronoUnit.MILLIS)

            repo.retry = ApiResult.Failure(ApiFailure.LlmBudgetExceeded(900))
            vm.retry()
            advanceUntilIdle()
            val s = vm.state.value
            val resetAt = s.budgetResetAt!!
            assertFalse(resetAt.isBefore(before.plusSeconds(900)))
            assertFalse(
                resetAt.isAfter(
                    java.time.Instant
                        .now()
                        .plusSeconds(900),
                ),
            )
            assertNull(s.actionError)
            assertNull(s.keyNotice)
            assertFalse(s.acting)

            repo.retry = ApiResult.Success(job(JobStatus.Queued))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.budgetResetAt)
        }

    @Test
    fun `a research refused for the key is a notice and is told to the monitor`() =
        runTest(dispatcher) {
            val keys = CreditTrackingMonitor()
            val repo = FakeResources()
            val vm = ResourcesViewModel("t1", repo, keys)
            vm.clock = { dispatcherTime(vm) }
            repo.view = ApiResult.Success(view(job = job(JobStatus.Failed)))
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()

            repo.retry = ApiResult.Failure(ApiFailure.LlmKeyRejected)
            vm.retry()
            advanceUntilIdle()
            assertEquals(
                LlmKeyVariant.Rejected,
                vm.state.value.keyNotice
                    ?.variant,
            )

            repo.retry = ApiResult.Failure(ApiFailure.LlmKeyMissing)
            vm.retry()
            advanceUntilIdle()
            assertEquals(
                LlmKeyVariant.Missing,
                vm.state.value.keyNotice
                    ?.variant,
            )
            assertEquals(listOf(LlmKeyVariant.Rejected, LlmKeyVariant.Missing), keys.observed)
        }

    @Test
    fun `a research refused for the credit is a credit notice, repeated when it fails again`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Failed)))

            repo.retry = ApiResult.Failure(ApiFailure.LlmCreditExhausted)
            vm.retry()
            advanceUntilIdle()
            val first = vm.state.value
            assertEquals(LlmKeyVariant.Credit, first.keyNotice?.variant)
            assertNull(first.actionError)
            assertFalse(first.acting)

            vm.retry()
            // The panel stays while it goes out again.
            assertEquals(
                LlmKeyVariant.Credit,
                vm.state.value.keyNotice
                    ?.variant,
            )
            assertTrue(vm.state.value.acting)
            advanceUntilIdle()
            assertTrue(
                vm.state.value.keyNotice!!
                    .repeated,
            )

            repo.retry = ApiResult.Success(job(JobStatus.Queued))
            vm.retry()
            advanceUntilIdle()
            assertNull(vm.state.value.keyNotice)
        }

    @Test
    fun `start research shows the new job and a 409 only reloads`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val done = view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 1, Instant.EPOCH))
            val vm = started(repo, done)
            vm.startResearch()
            advanceUntilIdle()
            assertEquals(
                "j2",
                vm.state.value.view
                    ?.job
                    ?.id,
            )
            assertEquals(
                1,
                vm.state.value.view
                    ?.resources
                    ?.size,
            )

            repo.start = ApiResult.Failure(ApiFailure.Conflict)
            repo.view = ApiResult.Success(done.copy(job = running()))
            val before = repo.getCalls
            vm.startResearch()
            advanceUntilIdle()
            assertEquals(2, repo.startCalls)
            assertTrue(repo.getCalls > before)
            assertNull(vm.state.value.actionError)
        }

    @Test
    fun `a second tap while the call is in flight is ignored`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm =
                started(repo, view(job = job(JobStatus.Succeeded), research = ResearchInfo("j1", 1, Instant.EPOCH)))
            vm.startResearch()
            vm.startResearch()
            advanceUntilIdle()
            assertEquals(1, repo.startCalls)
        }

    @Test
    fun `a failed fetch after a success does not leave the panel unsettled`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(listOf(source("a")), running(), ResearchInfo("old", 1, Instant.EPOCH)))
            repo.view = ApiResult.Failure(network)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded)))
            // A resync lands while the reload is still going: it must not strand the state.
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertTrue(vm.state.value.settled)
        }

    @Test
    fun `a coalesced progress update never undoes a job that finished in between`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = running("searching")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "searching", message = "Primo")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "searching", message = "Secondo")))
            repo.view =
                ApiResult.Success(
                    view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 1, Instant.EPOCH)),
                )
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertEquals(
                JobStatus.Succeeded,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `a replayed running after succeeded changes nothing and does not repeat the effect`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = running()))
            repo.view =
                ApiResult.Success(
                    view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 1, Instant.EPOCH)),
                )
            val effects = mutableListOf<ResourcesEffect>()
            val collector = launchCollect(vm, effects)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded)))
            advanceUntilIdle()
            vm.onEvent(ChatEvent.JobUpdated(running()))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded)))
            advanceUntilIdle()
            assertEquals(
                JobStatus.Succeeded,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
            assertEquals(1, effects.size)
            collector.cancel()
        }

    @Test
    fun `a late http answer never takes the job back to queued`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val done = view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 1, Instant.EPOCH))
            val vm = started(repo, done)
            val created = Instant.parse("2026-11-09T15:00:00Z")
            // The stream already said "running" for the new job when the POST answer (queued) arrives.
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, id = "j2", step = "planning", created = created)))
            vm.startResearch()
            advanceUntilIdle()
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    @Test
    fun `a stale fetch cannot take a newer job back`() =
        runTest(dispatcher) {
            val repo = FakeResources()
            val vm = started(repo, view(job = job(JobStatus.Queued)))
            val stale = view(job = job(JobStatus.Queued))
            repo.view = ApiResult.Success(stale)
            // Every fetch is overtaken by a newer event, so the repeats run out.
            repo.onGet = { vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, step = "reading"))) }
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            repo.onGet = null
            assertEquals(
                JobStatus.Running,
                vm.state.value.view
                    ?.job
                    ?.status,
            )
        }

    private fun TestScope.launchCollect(
        vm: ResourcesViewModel,
        into: MutableList<ResourcesEffect>,
    ) = backgroundScope.launchCollecting(vm, into)
}

private fun CoroutineScope.launchCollecting(
    vm: ResourcesViewModel,
    into: MutableList<ResourcesEffect>,
) = launch(Dispatchers.Unconfined) { vm.effects.collect { into += it } }
