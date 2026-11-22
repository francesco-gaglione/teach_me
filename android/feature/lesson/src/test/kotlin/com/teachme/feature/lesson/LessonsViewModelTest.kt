package com.teachme.feature.lesson

import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.Page
import com.teachme.core.model.TopicState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class LessonsViewModelTest {
    private val backend = FakeLessons()
    private val store = FakeStore()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val keys = CreditTrackingMonitor()

    private fun vm() = LessonsViewModel("T1", backend, store, keys)

    private fun page(
        vararg lessons: com.teachme.core.model.Lesson,
        next: String? = null,
    ) = ApiResult.Success(Page(lessons.toList(), next))

    private fun generation(
        status: JobStatus,
        step: String? = null,
        message: String? = null,
        done: Int? = null,
        errorCode: String? = null,
        result: JobResult? = null,
        createdSecond: Long = 1,
    ) = job(
        "lesson_generation",
        status,
        id = "G1",
        step = step,
        message = message,
        done = done,
        errorCode = errorCode,
        result = result,
        createdSecond = createdSecond,
    )

    // --- the card --------------------------------------------------------------------------------------------------

    @Test
    fun `the card follows the topic state, the last lesson and the job`() {
        val done = lesson("L2", LessonStatus.Completed, 2)
        val open = lesson("L3", LessonStatus.InProgress, 3)
        assertEquals(
            LessonsCard.Blocked(TopicState.Interviewing, false),
            cardFor(TopicState.Interviewing, false, emptyList(), null),
        )
        assertEquals(
            LessonsCard.Blocked(TopicState.Researching, true),
            cardFor(TopicState.Researching, true, emptyList(), null),
        )
        assertEquals(LessonsCard.Free(false), cardFor(TopicState.Ready, false, emptyList(), null))
        assertEquals(LessonsCard.Free(true), cardFor(TopicState.Ready, false, listOf(done), null))
        assertEquals(LessonsCard.Unfinished(open), cardFor(TopicState.Ready, false, listOf(open, done), null))
        assertEquals(
            LessonsCard.Unfinished(lesson("L1", LessonStatus.NotStarted, 1)),
            cardFor(TopicState.Ready, false, listOf(lesson("L1", LessonStatus.NotStarted, 1)), null),
        )
        val running = generation(JobStatus.Running, "writing")
        assertEquals(LessonsCard.Preparing(running), cardFor(TopicState.Ready, false, listOf(done), running))
        val failed = generation(JobStatus.Failed, errorCode = "invalid_output")
        assertEquals(LessonsCard.Failed(failed), cardFor(TopicState.Ready, false, listOf(done), failed))
        // A finished job is history: the card goes back to offering the next lesson.
        assertEquals(
            LessonsCard.Free(true),
            cardFor(TopicState.Ready, false, listOf(done), generation(JobStatus.Succeeded)),
        )
    }

    @Test
    fun `the phases are in words, one current, and a repair puts the check back`() {
        val planning = generationPhases(generation(JobStatus.Running, "planning"), false)
        assertEquals(
            listOf(PhaseState.Current, PhaseState.Todo, PhaseState.Todo, PhaseState.Todo),
            planning.map { it.second },
        )
        val checking = generationPhases(generation(JobStatus.Running, "checking"), false)
        assertEquals(
            listOf(PhaseState.Done, PhaseState.Done, PhaseState.Current, PhaseState.Todo),
            checking.map { it.second },
        )
        assertTrue(generationPhases(generation(JobStatus.Succeeded), true).all { it.second == PhaseState.Done })
        // Unknown step: the first phase is shown as current, never a crash.
        assertEquals(
            PhaseState.Current,
            generationPhases(generation(JobStatus.Running, "something_new"), false).first().second,
        )
        assertFalse(generation(JobStatus.Running, "writing", "Correggo la lezione").isRepair())
        assertTrue(generation(JobStatus.Running, "writing", null, done = 2).isRepair())
        assertFalse(generation(JobStatus.Running, "writing", "Scrivo", done = 1).isRepair())
        assertFalse(generation(JobStatus.Running, "checking", "Correggo").isRepair())
    }

    // --- loading ---------------------------------------------------------------------------------------------------

    @Test
    fun `the first load reads the lessons and the current generation job`() =
        runTest {
            backend.lessonPages = mapOf(null to page(lesson("L3", number = 3), lesson("L2", LessonStatus.Completed, 2)))
            backend.jobs =
                ApiResult.Success(
                    listOf(
                        generation(JobStatus.Succeeded, createdSecond = 5),
                        job("lesson_completion", JobStatus.Failed, id = "C2", lessonId = "L2", createdSecond = 4),
                    ),
                )
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            val s = vm.state.value
            assertEquals(LessonsLoad.Ready, s.load)
            assertEquals(listOf("L3", "L2"), s.lessons.map { it.id })
            assertEquals(JobStatus.Succeeded, s.job?.status)
            assertEquals(JobStatus.Failed, s.completionJobs["L2"]?.status)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a failed first load shows the error and Riprova loads again, a failed refresh keeps the list`() =
        runTest {
            backend.lessonPages = mapOf(null to ApiResult.Failure(ApiFailure.Unauthorized))
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            assertEquals(LessonsLoad.Failed(LoadError.Unauthorized), vm.state.value.load)
            backend.lessonPages = mapOf(null to page(lesson("L1", number = 1)))
            vm.retryLoad()
            runCurrent()
            assertEquals(LessonsLoad.Ready, vm.state.value.load)
            backend.lessonPages =
                mapOf(null to ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout)))
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            assertEquals(LessonsLoad.Ready, vm.state.value.load)
            assertEquals(1, vm.state.value.lessons.size)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `the list is paginated and the local reading position comes from the device`() =
        runTest {
            backend.lessonPages =
                mapOf(
                    null to page(lesson("L3", LessonStatus.InProgress, 3), next = "c2"),
                    "c2" to page(lesson("L2", LessonStatus.Completed, 2)),
                )
            store.stored =
                com.teachme.sdui.logic
                    .LessonSavedState(sectionNumber = 2, sectionTotal = 4)
                    .encode()
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            assertEquals("c2", vm.state.value.nextCursor)
            assertEquals(2 to 4, vm.state.value.sectionProgress["L3"])
            vm.loadMore()
            runCurrent()
            assertEquals(
                listOf("L3", "L2"),
                vm.state.value.lessons
                    .map { it.id },
            )
            assertNull(vm.state.value.nextCursor)
            assertEquals(listOf<String?>(null, "c2"), backend.listedCursors)
            vm.loadMore()
            runCurrent()
            assertEquals("no cursor, no request", 2, backend.listedCursors.size)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a page that fails can be asked again`() =
        runTest {
            backend.lessonPages =
                mapOf(
                    null to page(lesson("L3", number = 3), next = "c2"),
                    "c2" to ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout)),
                )
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.loadMore()
            runCurrent()
            assertTrue(vm.state.value.pageFailed)
            backend.lessonPages = backend.lessonPages + ("c2" to page(lesson("L2", number = 2)))
            vm.loadMore()
            runCurrent()
            assertFalse(vm.state.value.pageFailed)
            assertEquals(2, vm.state.value.lessons.size)
            vm.viewModelScope.cancel()
        }

    // --- generating ------------------------------------------------------------------------------------------------

    @Test
    fun `Genera lezione starts a job, the events move it along and the end reloads the list with a toast effect`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.generate()
            assertTrue(vm.state.value.starting)
            runCurrent()
            assertEquals(1, backend.startCalls)
            assertFalse(vm.state.value.starting)
            assertEquals(
                JobStatus.Queued,
                vm.state.value.job
                    ?.status,
            )

            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Running, "writing", "Scrivo la sezione 2")))
            assertEquals(
                "writing",
                vm.state.value.job
                    ?.progress
                    ?.step,
            )
            // A replayed older event never takes the job back.
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Queued)))
            assertEquals(
                JobStatus.Running,
                vm.state.value.job
                    ?.status,
            )

            backend.lessonPages = mapOf(null to page(lesson("L1", LessonStatus.NotStarted, 1)))
            vm.effects.test {
                vm.onEvent(
                    ChatEvent.JobUpdated(
                        generation(JobStatus.Succeeded, result = JobResult(lessonId = "L1", lessonNumber = 1)),
                    ),
                )
                runCurrent()
                assertEquals(LessonsEffect.Ready(1, "G1"), awaitItem())
                cancelAndIgnoreRemainingEvents()
            }
            assertEquals(
                listOf("L1"),
                vm.state.value.lessons
                    .map { it.id },
            )
            assertEquals("L1", vm.state.value.newLessonId)
            assertTrue(
                vm.state.value.announcement
                    ?.ready == true,
            )
            vm.markSeen()
            assertNull(vm.state.value.newLessonId)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a failed generation says so once and Riprova reuses the same job`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Running, "checking")))
            vm.effects.test {
                vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "invalid_output")))
                assertEquals(LessonsEffect.Failed("G1"), awaitItem())
                // The same event again (a replay) does not repeat the toast.
                vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "invalid_output")))
                expectNoEvents()
                cancelAndIgnoreRemainingEvents()
            }
            backend.retryResult = ApiResult.Success(generation(JobStatus.Queued))
            vm.retry()
            assertTrue(vm.state.value.retrying)
            runCurrent()
            assertEquals(1, backend.retryCalls)
            assertEquals(
                JobStatus.Queued,
                vm.state.value.job
                    ?.status,
            )
            assertFalse(vm.state.value.retrying)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `an older generation job stays known by id when a newer one takes the current place`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "timeout")))
            val newer = job("lesson_generation", JobStatus.Queued, id = "G2", createdSecond = 5)
            vm.onEvent(ChatEvent.JobUpdated(newer))
            // A late event of the old job does not take the current place back, but its card can still read it.
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "timeout")))

            val s = vm.state.value
            assertEquals("G2", s.job?.id)
            assertEquals(JobStatus.Failed, s.generationJob("G1")?.status)
            assertEquals(JobStatus.Queued, s.generationJob("G2")?.status)
            assertNull(s.generationJob("G9"))
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a REST list with the job running overwrites a failed state, for the current job and the map`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "timeout")))
            assertEquals(
                JobStatus.Failed,
                vm.state.value.job
                    ?.status,
            )
            // Another client retried: the server's list says running, and that is the truth.
            backend.jobs = ApiResult.Success(listOf(generation(JobStatus.Running, "writing")))
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            val s = vm.state.value
            assertEquals(JobStatus.Running, s.job?.status)
            assertEquals(JobStatus.Running, s.generationJobs["G1"]?.status)
            assertEquals(JobStatus.Running, s.generationJob("G1")?.status)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `canRetry of a failed card follows the last job when it changes`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Failed, errorCode = "timeout")))

            fun cardG1() =
                vm.state.value.let { s ->
                    lessonRequestState(s.generationJob("G1"), s.job?.id) { null }
                }
            assertEquals(LessonRequestState.Failed("timeout", canRetry = true), cardG1())
            vm.onEvent(ChatEvent.JobUpdated(job("lesson_generation", JobStatus.Queued, id = "G2", createdSecond = 5)))
            assertEquals(LessonRequestState.Failed("timeout", canRetry = false), cardG1())
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a generation refused for the key shows the key notice and creates nothing`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            backend.started = ApiResult.Failure(ApiFailure.LlmKeyMissing)

            vm.generate()
            runCurrent()

            val s = vm.state.value
            assertEquals(LlmKeyVariant.Missing, s.keyNotice?.variant)
            assertNull(s.actionError)
            assertNull(s.job)
            assertFalse(s.starting)

            // Trying again clears the notice first.
            backend.started = ApiResult.Success(generation(JobStatus.Queued))
            vm.generate()
            runCurrent()
            assertNull(vm.state.value.keyNotice)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a generation or a retry refused for the daily budget carries the reset and shows no generic error`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            val before =
                java.time.Instant
                    .now()
                    .truncatedTo(java.time.temporal.ChronoUnit.MILLIS)
            backend.started = ApiResult.Failure(ApiFailure.LlmBudgetExceeded(1800))

            vm.generate()
            runCurrent()

            val s = vm.state.value
            val resetAt = s.budgetResetAt!!
            assertFalse(resetAt.isBefore(before.plusSeconds(1800)))
            assertFalse(
                resetAt.isAfter(
                    java.time.Instant
                        .now()
                        .plusSeconds(1800),
                ),
            )
            assertNull(s.actionError)
            assertNull(s.keyNotice)
            assertNull(s.job)
            assertFalse(s.starting)

            // Without Retry-After it is the next midnight UTC; a success forgets the wait.
            backend.started = ApiResult.Failure(ApiFailure.LlmBudgetExceeded(null))
            vm.generate()
            runCurrent()
            assertEquals(
                0,
                vm.state.value.budgetResetAt!!
                    .atZone(java.time.ZoneOffset.UTC)
                    .toLocalTime()
                    .toSecondOfDay(),
            )
            backend.started = ApiResult.Success(generation(JobStatus.Queued))
            vm.generate()
            runCurrent()
            assertNull(vm.state.value.budgetResetAt)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a generation refused for the key is a notice and is told to the monitor`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()

            backend.started = ApiResult.Failure(ApiFailure.LlmKeyRejected)
            vm.generate()
            runCurrent()
            assertEquals(
                LlmKeyVariant.Rejected,
                vm.state.value.keyNotice
                    ?.variant,
            )

            backend.started = ApiResult.Failure(ApiFailure.LlmKeyMissing)
            vm.generate()
            runCurrent()
            assertEquals(
                LlmKeyVariant.Missing,
                vm.state.value.keyNotice
                    ?.variant,
            )
            assertEquals(listOf(LlmKeyVariant.Rejected, LlmKeyVariant.Missing), keys.observed)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `a generation refused for the credit keeps its notice while the retry goes out, and clears on success`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            backend.started = ApiResult.Failure(ApiFailure.LlmCreditExhausted)

            vm.generate()
            runCurrent()
            val first = vm.state.value.keyNotice!!
            assertEquals(LlmKeyVariant.Credit, first.variant)
            assertNull(vm.state.value.actionError)
            assertNull(vm.state.value.job)

            vm.generate()
            // The panel is still there while "Riprovo..." runs.
            assertEquals(
                LlmKeyVariant.Credit,
                vm.state.value.keyNotice
                    ?.variant,
            )
            assertTrue(vm.state.value.starting)
            runCurrent()
            assertTrue(
                vm.state.value.keyNotice!!
                    .repeated,
            )

            backend.started = ApiResult.Success(generation(JobStatus.Queued))
            vm.generate()
            runCurrent()
            assertNull(vm.state.value.keyNotice)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `409 on start looks at the truth in silence and a network error shows a message under the button`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            backend.started = ApiResult.Failure(ApiFailure.Conflict)
            backend.jobs = ApiResult.Success(listOf(generation(JobStatus.Running, "planning")))
            vm.generate()
            runCurrent()
            assertNull(vm.state.value.actionError)
            assertEquals(
                JobStatus.Running,
                vm.state.value.job
                    ?.status,
            )

            val other = vm()
            other.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            backend.started = ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout))
            other.generate()
            runCurrent()
            assertEquals(LoadError.Network, other.state.value.actionError)
            assertFalse(other.state.value.starting)
            vm.viewModelScope.cancel()
            other.viewModelScope.cancel()
        }

    @Test
    fun `closing jobs update the list flag and events of other topics are ignored`() =
        runTest {
            backend.lessonPages = mapOf(null to page(lesson("L2", LessonStatus.Completed, 2)))
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(
                ChatEvent.JobUpdated(
                    job("lesson_completion", JobStatus.Failed, id = "C9", lessonId = "L2", errorCode = "timeout"),
                ),
            )
            assertEquals(
                JobStatus.Failed,
                vm.state.value.completionJobs["L2"]
                    ?.status,
            )
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Running, "planning").copy(topicId = "other")))
            assertNull(vm.state.value.job)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `without a stream the first load still happens after a short grace`() =
        runTest {
            backend.lessonPages = mapOf(null to page(lesson("L1", number = 1)))
            val vm = vm()
            vm.loadIfStreamSilent()
            advanceTimeBy(2_000)
            assertEquals(LessonsLoad.Loading, vm.state.value.load)
            advanceTimeBy(1_500)
            runCurrent()
            assertEquals(LessonsLoad.Ready, vm.state.value.load)
            vm.viewModelScope.cancel()
        }

    @Test
    fun `glossary letters and search ignore accents and case`() {
        assertEquals("E", glossaryInitial("égalité"))
        assertEquals("#", glossaryInitial("3D"))
        assertEquals("O", glossaryInitial("ownership"))
        assertTrue(matchesQuery(term("ownership"), "OWNER"))
        assertTrue(matchesQuery(term("a", "Università"), "universita"))
        assertTrue(matchesQuery(term("x").copy(definition = "Il Proprietario"), "proprietario"))
        assertFalse(matchesQuery(term("ownership"), "borrow"))
    }

    @Test
    fun `a late REST answer does not take back what the stream already said`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            backend.started = ApiResult.Success(generation(JobStatus.Queued))
            vm.generate()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Running, "writing")))
            runCurrent()
            assertEquals(
                JobStatus.Running,
                vm.state.value.job
                    ?.status,
            )
            vm.viewModelScope.cancel()
        }

    @Test
    fun `the reload after a finished generation is tried again once if it fails`() =
        runTest {
            backend.lessonPages = mapOf(null to page())
            val vm = vm()
            vm.onEvent(ChatEvent.ResyncRequired)
            runCurrent()
            vm.onEvent(ChatEvent.JobUpdated(generation(JobStatus.Running, "saving")))
            backend.lessonPages =
                mapOf(null to ApiResult.Failure(ApiFailure.Network(com.teachme.core.model.UnreachableReason.Timeout)))
            vm.onEvent(
                ChatEvent.JobUpdated(
                    generation(JobStatus.Succeeded, result = JobResult(lessonId = "L1", lessonNumber = 1)),
                ),
            )
            runCurrent()
            assertTrue(
                vm.state.value.lessons
                    .isEmpty(),
            )
            backend.lessonPages = mapOf(null to page(lesson("L1", LessonStatus.NotStarted, 1)))
            advanceTimeBy(3_500)
            runCurrent()
            assertEquals(
                listOf("L1"),
                vm.state.value.lessons
                    .map { it.id },
            )
            vm.viewModelScope.cancel()
        }
}
