package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicTabNames
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.LessonSavedState
import com.teachme.sdui.logic.Outcome
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestResult
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Remembers the credit like the real monitor: `noteObserved(Credit)` sets the flag, `noteWorked` clears it. */
internal class CreditTrackingMonitor : LlmKeyMonitor by NoLlmKeyMonitor {
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

/** The lesson screen against a backend: loading, the attempt queue, the open answer and the closing (lesson.md 7, 8.1). */
@OptIn(ExperimentalCoroutinesApi::class)
class LessonBackendTest {
    private val backend = FakeLessons()
    private val queue = MemoryQueue()
    private val store = FakeStore()
    private val inbox = InMemoryTopicTabInbox()
    private val clock = ImmediateDispatchers()

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private val created = mutableListOf<LessonViewModel>()

    /** `runTest` waits for every pending task: the polling and retry loops of the screens must be stopped first. */
    private fun vmTest(body: suspend TestScope.() -> Unit): TestResult =
        runTest {
            try {
                body()
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private val keys = CreditTrackingMonitor()

    private fun vm(
        syncQueue: MemoryQueue = queue,
        repo: FakeLessons = backend,
    ) = LessonViewModel(
        SavedStateHandle(mapOf("key" to "L1")),
        FakeSource(null),
        store,
        FakeLogger(),
        clock,
        repo,
        AttemptSyncer(
            repo,
            syncQueue,
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        HighlightSyncer(
            repo,
            MemoryHighlightStore(),
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        StrokeSyncer(
            repo,
            MemoryStrokeStore(),
            com.teachme.core.model
                .ManualUserScopes(),
        ),
        syncQueue,
        inbox,
        keys,
    ).also { created += it }

    private fun ready(vm: LessonViewModel) = vm.state.value as LessonUiState.Ready

    private suspend fun TestScope.opened(
        syncQueue: MemoryQueue = queue,
        repo: FakeLessons = backend,
    ): LessonViewModel {
        val vm = vm(syncQueue, repo)
        runCurrent()
        return vm
    }

    private fun answerQuiz(
        vm: LessonViewModel,
        option: String = "a",
    ) {
        vm.updateExercise("q1") { ExerciseReducers.selectSingle(it, option) }
        vm.updateExercise("q1") {
            ExerciseReducers.checkQuizSingle(
                com.teachme.sdui.model
                    .QuizSingle("q1", question = "?", options = emptyList(), correct = "a"),
                it,
                5,
            )
        }
    }

    private fun typeAndSend(
        vm: LessonViewModel,
        text: String = "Perché è stata spostata.",
    ) {
        vm.updateExercise("oa") { ExerciseReducers.setAnswer(it, text, 300) }
        vm.updateExercise("oa") { ExerciseReducers.submit(it) }
    }

    /** Reveals the section with the exercises. */
    private fun reveal(vm: LessonViewModel) = vm.continueLesson()

    // --- loading ---------------------------------------------------------------------------------------------------

    @Test
    fun `a backend lesson opens from the document and restores the outcomes of the backend`() =
        vmTest {
            backend.store[Triple("L1", "q1", 1)] =
                saved(queued("q1", "quiz_single", buildJsonObject { put("option", "b") }, "incorrect"))
            backend.store[Triple("L1", "oa", 1)] =
                saved(
                    queued(
                        "oa",
                        "open_answer",
                        buildJsonObject {
                            put("text", "Boh")
                        },
                    ),
                    AttemptOutcome.Partial,
                    "Quasi: manca il perché.",
                )
            val vm = opened()
            val r = ready(vm)
            assertEquals("L1", r.lesson?.id)
            val q1 = r.exercise("q1")
            assertTrue(q1.checked)
            assertEquals(Outcome.Incorrect, q1.outcome)
            val oa = r.exercise("oa")
            assertTrue(oa.graded)
            assertEquals("Quasi: manca il perché.", oa.feedback)
            assertFalse("a lesson in progress opens on the lesson", r.showSummary)
        }

    @Test
    fun `load failures give the right states`() =
        vmTest {
            backend.view = ApiResult.Failure(NETWORK_DOWN)
            assertEquals(LessonUiState.LoadFailed, opened().state.value)
            backend.view = ApiResult.Failure(ApiFailure.NotFound)
            assertEquals(LessonUiState.Gone, opened().state.value)
            backend.view = ApiResult.Success(LessonView(lesson(), "{not json", emptyList(), null))
            assertEquals(LessonUiState.Unusable, opened().state.value)
        }

    @Test
    fun `a completed lesson opens on the summary and is read only`() =
        vmTest {
            backend.completed(
                job("lesson_completion", JobStatus.Succeeded, id = "C1", lessonId = "L1"),
                listOf(record()),
                listOf(term("ownership")),
                3,
            )
            val r = ready(opened())
            assertTrue(r.showSummary)
            assertTrue(r.readOnly)
            val done = r.completion as CompletionUi.Done
            assertEquals(1, done.records.size)
            assertEquals(3, done.reviewCardCount)
            assertEquals(r.sections.sections.size, r.saved.revealedCount)
        }

    @Test
    fun `a lesson already downloaded opens without a network and catches up when the backend answers`() =
        vmTest {
            opened()
            val offline = FakeLessons().apply { view = ApiResult.Failure(NETWORK_DOWN) }
            val vm = opened(repo = offline)
            val r = ready(vm)
            assertEquals("L1", r.lesson?.id)
            assertEquals("Ownership", r.document.title)
            // The attempts of the backend arrive when it is reachable again.
            offline.view = ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null))
            offline.store[Triple("L1", "q1", 1)] =
                saved(queued("q1", "quiz_single", buildJsonObject { put("option", "a") }, "correct"))
            vm.onResume()
            runCurrent()
            assertTrue(ready(vm).exercise("q1").done)
        }

    @Test
    fun `the content language comes from the lesson payload`() =
        vmTest {
            backend.view =
                ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null, language = TopicLanguage.En))
            assertEquals("en", ready(opened()).contentLanguage)
        }

    @Test
    fun `a lesson without a language is Italian and an unknown one follows the device language`() =
        vmTest {
            assertEquals("it", ready(opened()).contentLanguage)
            backend.view =
                ApiResult.Success(
                    LessonView(lesson(), SMALL_LESSON, emptyList(), null, language = TopicLanguage.Unknown),
                )
            assertEquals(contentLanguageOf(TopicLanguage.Unknown), ready(opened()).contentLanguage)
            assertEquals(
                java.util.Locale
                    .getDefault()
                    .language,
                contentLanguageOf(TopicLanguage.Unknown),
            )
        }

    @Test
    fun `a cached lesson keeps its language when opened offline`() =
        vmTest {
            backend.view =
                ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null, language = TopicLanguage.En))
            opened()
            val offline = FakeLessons().apply { view = ApiResult.Failure(NETWORK_DOWN) }
            assertEquals("en", ready(opened(repo = offline)).contentLanguage)
        }

    @Test
    fun `a lesson never downloaded and unreachable shows the load error`() =
        vmTest {
            val offline = FakeLessons().apply { view = ApiResult.Failure(NETWORK_DOWN) }
            assertEquals(LessonUiState.LoadFailed, opened(repo = offline).state.value)
        }

    // --- the queue -------------------------------------------------------------------------------------------------

    @Test
    fun `a checked exercise is queued and sent without waiting, and the queue empties`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            assertEquals(1, backend.requests.size)
            val sent = backend.requests.single().single()
            assertEquals("q1", sent.blockId)
            assertEquals(1, sent.attemptNumber)
            assertEquals("correct", sent.outcome)
            assertEquals(0, queue.size)
            assertEquals(0, ready(vm).queued)
            // "Continua" never depends on the send: the quiz was already answered locally.
            assertTrue(ready(vm).exercise("q1").done)
        }

    @Test
    fun `answers given offline stay queued and go out later, once`() =
        vmTest {
            backend.failures += NETWORK_DOWN
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            assertEquals(1, queue.size)
            assertEquals(1, ready(vm).queued)
            assertTrue("the exercise counts as done even if the send failed", ready(vm).exercise("q1").done)
            advanceTimeBy(clock.retryMs + 1)
            runCurrent()
            assertEquals(0, queue.size)
            assertEquals(2, backend.requests.size)
            assertEquals(1, backend.store.size)
        }

    @Test
    fun `the queue of a previous run is sent when the lesson opens, even after a restart`() =
        vmTest {
            // A first run answered offline and was killed: only the persistent queue remains.
            val offline = FakeLessons().apply { failures += NETWORK_DOWN }
            val first = vm(queue, offline)
            runCurrent()
            reveal(first)
            answerQuiz(first)
            runCurrent()
            assertEquals(1, queue.size)
            first.flushState()

            val second = opened(queue, backend)
            runCurrent()
            assertEquals(0, queue.size)
            assertEquals(
                "q1",
                backend.requests
                    .single()
                    .single()
                    .blockId,
            )
            assertTrue(ready(second).exercise("q1").done)
        }

    @Test
    fun `resending an attempt never duplicates it on the backend`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            // The same pair arrives again (a lost answer, an app restart): the backend keeps the first.
            val again = queued("q1", "quiz_single", buildJsonObject { put("option", "b") }, "incorrect")
            queue.enqueue(again)
            vm.retryQueueNow()
            runCurrent()
            assertEquals(1, backend.store.size)
            assertEquals(
                AttemptOutcome.Correct,
                backend.store.values
                    .single()
                    .outcome,
            )
            assertEquals(0, queue.size)
        }

    @Test
    fun `a rejected attempt is dropped without a word and the exercise stays answered`() =
        vmTest {
            backend.failures +=
                ApiFailure.Validation(
                    listOf(
                        com.teachme.core.model
                            .ApiErrorDetail("attempts[0].answer", "invalid"),
                    ),
                )
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            assertEquals(0, queue.size)
            assertTrue(ready(vm).exercise("q1").done)
        }

    @Test
    fun `a 409 on the attempts empties the queue and shows the lesson as closed elsewhere`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            backend.failures += ApiFailure.Conflict
            backend.completed(job("lesson_completion", JobStatus.Running, id = "C1", lessonId = "L1", step = "grading"))
            answerQuiz(vm)
            runCurrent()
            assertEquals(0, queue.size)
            val r = ready(vm)
            assertTrue(r.closedElsewhere)
            assertTrue(r.readOnly)
            assertTrue(r.showSummary)
            assertTrue(r.completion is CompletionUi.Running)
        }

    // --- the open answer -------------------------------------------------------------------------------------------

    @Test
    fun `an open answer is evaluated inside the request and shows the verdict`() =
        vmTest {
            backend.grade = { AttemptOutcome.Partial to "Giusto che si sposta; manca il *perché*." }
            val vm = opened()
            reveal(vm)
            typeAndSend(vm)
            // Sent at once and counted as done while the Grader works.
            assertTrue(ready(vm).exercise("oa").evaluating)
            assertTrue(ready(vm).exercise("oa").done)
            runCurrent()
            val oa = ready(vm).exercise("oa")
            assertFalse(oa.evaluating)
            assertTrue(oa.graded)
            assertEquals(Outcome.Partial, oa.outcome)
            assertEquals(
                "Perché è stata spostata.",
                backend.requests
                    .single()
                    .single()
                    .answer["text"]
                    ?.toString()
                    ?.trim('"'),
            )
        }

    @Test
    fun `a pending answer counts as done, is graded once on resume and by hand with Valuta ora`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            typeAndSend(vm)
            runCurrent()
            var oa = ready(vm).exercise("oa")
            assertTrue(oa.serverPending)
            assertFalse(oa.graded)
            assertTrue("a pending answer never blocks \"Continua\"", oa.done)

            // Back in the foreground: one automatic second chance (the Grader answers this time).
            backend.grade = { AttemptOutcome.Correct to "Corretto: hai spiegato il move." }
            vm.onResume()
            runCurrent()
            oa = ready(vm).exercise("oa")
            assertTrue(oa.graded)
            assertEquals(2, backend.requests.size)
            assertFalse(oa.serverPending)
        }

    @Test
    fun `the automatic retry is used once, then only the button re-sends`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            typeAndSend(vm)
            runCurrent()
            vm.onResume()
            runCurrent()
            assertEquals(2, backend.requests.size)
            // Still pending: a second resume must not call the Grader again (each call costs money).
            vm.onResume()
            runCurrent()
            assertEquals(2, backend.requests.size)
            vm.reevaluate("oa")
            runCurrent()
            assertEquals(3, backend.requests.size)
            val oa = ready(vm).exercise("oa")
            assertTrue(oa.serverPending)
            assertTrue("\"Non è ancora andata a buon fine\"", oa.reevalFailed)
        }

    @Test
    fun `without network an open answer is registered locally and graded when the queue goes out`() =
        vmTest {
            backend.failures += NETWORK_DOWN
            val vm = opened()
            reveal(vm)
            typeAndSend(vm)
            runCurrent()
            var oa = ready(vm).exercise("oa")
            assertFalse(oa.evaluating)
            assertFalse("only in the local queue, not on the server", oa.serverPending)
            assertTrue(oa.done)
            assertEquals(1, queue.size)
            backend.grade = { AttemptOutcome.Correct to "Corretto." }
            advanceTimeBy(clock.retryMs + 1)
            runCurrent()
            oa = ready(vm).exercise("oa")
            assertTrue(oa.graded)
            assertEquals(0, queue.size)
        }

    @Test
    fun `a refused open answer says it was saved and not evaluated`() =
        vmTest {
            backend.failures += ApiFailure.Validation(emptyList())
            val vm = opened()
            reveal(vm)
            typeAndSend(vm)
            runCurrent()
            val oa = ready(vm).exercise("oa")
            assertTrue(oa.notEvaluable)
            assertTrue(oa.done)
            assertEquals(0, queue.size)
        }

    // --- completing ------------------------------------------------------------------------------------------------

    private fun TestScope.finishLesson(vm: LessonViewModel) {
        reveal(vm)
        answerQuiz(vm)
        typeAndSend(vm)
        runCurrent()
    }

    @Test
    fun `complete lesson sends the answers, closes the lesson and follows the job until the records arrive`() =
        vmTest {
            backend.grade = { AttemptOutcome.Correct to "Corretto." }
            val vm = opened()
            finishLesson(vm)
            vm.continueLesson()
            assertTrue("the button shows \"Invio le tue risposte\" first", ready(vm).completing)
            runCurrent()
            var r = ready(vm)
            assertEquals(1, backend.completeCalls)
            assertTrue(r.showSummary)
            assertTrue(r.saved.completed)
            assertEquals(LessonStatus.Completed, r.lesson?.status)
            assertTrue(r.completion is CompletionUi.Running)
            assertEquals(0, queue.size)

            // The job runs: a poll every 3 s reads the progress, then the result.
            backend.completed(
                job("lesson_completion", JobStatus.Running, id = "C1", lessonId = "L1", step = "recording"),
            )
            advanceTimeBy(clock.pollMs + 1)
            r = ready(vm)
            assertEquals("recording", (r.completion as CompletionUi.Running).job?.progress?.step)
            backend.completed(
                job("lesson_completion", JobStatus.Succeeded, id = "C1", lessonId = "L1"),
                listOf(record(), record("r2", 2)),
                listOf(term("ownership")),
                cards = 4,
            )
            advanceTimeBy(clock.pollMs + 1)
            val done = ready(vm).completion as CompletionUi.Done
            assertEquals(2, done.records.size)
            assertEquals(1, done.terms.size)
            assertEquals(4, done.reviewCardCount)
            val polls = backend.getCalls
            // The polling stops with the job.
            advanceTimeBy(clock.pollMs * 3)
            assertEquals(polls, backend.getCalls)
        }

    @Test
    fun `a closing refused for the key keeps the lesson open, with a notice instead of the summary`() =
        vmTest {
            val vm = opened()
            finishLesson(vm)
            backend.complete = ApiResult.Failure(ApiFailure.LlmKeyRejected)

            vm.continueLesson()
            runCurrent()

            val r = ready(vm)
            assertFalse("the summary does not open", r.showSummary)
            assertFalse(r.saved.completed)
            assertEquals(CompletionUi.None, r.completion)
            assertEquals(LlmKeyVariant.Rejected, r.keyNotice?.variant)
            // The answers got there before the closing was refused, and nothing is waiting to be sent.
            assertEquals(0, queue.size)

            // Once the key is saved the button works again, and trying clears the notice.
            backend.complete = ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
            vm.continueLesson()
            runCurrent()
            assertNull(ready(vm).keyNotice)
            assertTrue(ready(vm).completion is CompletionUi.Running)
        }

    @Test
    fun `a closing refused for the credit keeps the lesson open, and the notice stays while it goes out again`() =
        vmTest {
            val vm = opened()
            finishLesson(vm)
            backend.complete = ApiResult.Failure(ApiFailure.LlmCreditExhausted)

            vm.continueLesson()
            runCurrent()

            val first = ready(vm)
            assertFalse(first.showSummary)
            assertEquals(CompletionUi.None, first.completion)
            assertEquals(LlmKeyVariant.Credit, first.keyNotice?.variant)
            assertFalse(first.keyNotice!!.repeated)

            // "Riprova" is the same action: still exhausted is announced, and the notice is a new one.
            vm.continueLesson()
            assertEquals(LlmKeyVariant.Credit, ready(vm).keyNotice?.variant)
            assertTrue(ready(vm).completing)
            runCurrent()
            val second = ready(vm).keyNotice!!
            assertTrue(second.repeated)
            assertTrue(second.id > first.keyNotice!!.id)

            backend.complete = ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
            vm.continueLesson()
            runCurrent()
            assertNull(ready(vm).keyNotice)
            assertTrue(ready(vm).completion is CompletionUi.Running)
        }

    @Test
    fun `a completion with nothing demonstrated is a legitimate empty result`() =
        vmTest {
            val vm = opened()
            finishLesson(vm)
            vm.continueLesson()
            runCurrent()
            backend.completed(job("lesson_completion", JobStatus.Succeeded, id = "C1", lessonId = "L1"))
            advanceTimeBy(clock.pollMs + 1)
            val done = ready(vm).completion as CompletionUi.Done
            assertTrue(done.isEmpty)
        }

    @Test
    fun `without network the summary opens anyway and the closing is sent when the connection returns`() =
        vmTest {
            val vm = opened()
            finishLesson(vm)
            backend.failures += NETWORK_DOWN
            // Offline: the first request of the answers fails (the queue was already empty, so the closing fails instead).
            backend.complete = ApiResult.Failure(NETWORK_DOWN)
            vm.continueLesson()
            runCurrent()
            var r = ready(vm)
            assertTrue(r.showSummary)
            assertTrue(r.saved.completed)
            assertEquals(CompletionUi.NeedsSend(), r.completion)
            // The connection is back: "Riprova ora".
            backend.complete = ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
            vm.retryCompletion()
            runCurrent()
            r = ready(vm)
            assertTrue(r.completion is CompletionUi.Running)
            assertEquals(2, backend.completeCalls)
        }

    @Test
    fun `answers that cannot be sent keep the lesson closing for later and still open the summary`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            backend.failures += NETWORK_DOWN
            answerQuiz(vm)
            runCurrent()
            backend.failures += NETWORK_DOWN
            vm.updateExercise("oa") { ExerciseReducers.setAnswer(it, "x", 300) }
            vm.updateExercise("oa") { ExerciseReducers.submit(it) }
            runCurrent()
            assertTrue(queue.size > 0)
            backend.failures += NETWORK_DOWN
            vm.continueLesson()
            runCurrent()
            val r = ready(vm)
            assertTrue(r.showSummary)
            assertEquals(CompletionUi.NeedsSend(), r.completion)
            assertEquals("the lesson is not closed on the backend yet", 0, backend.completeCalls)
        }

    @Test
    fun `a lesson completed here but not on the backend resumes its closing when reopened`() =
        vmTest {
            val first = opened()
            finishLesson(first)
            backend.complete = ApiResult.Failure(NETWORK_DOWN)
            first.continueLesson()
            runCurrent()
            first.flushState()
            // Killed. The backend lesson is still in progress, the device knows it was completed.
            backend.complete = ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
            val second = opened()
            runCurrent()
            assertTrue(ready(second).showSummary)
            assertTrue(ready(second).completion is CompletionUi.Running)
        }

    @Test
    fun `a 409 on complete is treated as done`() =
        vmTest {
            val vm = opened()
            finishLesson(vm)
            backend.complete = ApiResult.Failure(ApiFailure.Conflict)
            backend.completed(job("lesson_completion", JobStatus.Running, id = "C1", lessonId = "L1"))
            vm.continueLesson()
            runCurrent()
            val r = ready(vm)
            assertTrue(r.showSummary)
            assertEquals(LessonStatus.Completed, r.lesson?.status)
            assertTrue(r.completion is CompletionUi.Running)
        }

    @Test
    fun `a failed closing job offers Riprova and goes back to running`() =
        vmTest {
            backend.completed(
                job("lesson_completion", JobStatus.Failed, id = "C1", lessonId = "L1", errorCode = "invalid_output"),
            )
            val vm = opened()
            val failed = ready(vm).completion as CompletionUi.Failed
            assertEquals("invalid_output", failed.job.error?.code)
            vm.retryCompletionJob()
            assertTrue((ready(vm).completion as CompletionUi.Failed).retrying)
            runCurrent()
            assertEquals(1, backend.retryCalls)
            assertTrue(ready(vm).completion is CompletionUi.Running)

            // A network error on the retry keeps the panel with its own message.
            backend.completed(
                job("lesson_completion", JobStatus.Failed, id = "C1", lessonId = "L1", errorCode = "timeout"),
            )
            backend.retryResult = ApiResult.Failure(NETWORK_DOWN)
            val other = opened()
            other.retryCompletionJob()
            runCurrent()
            val again = ready(other).completion as CompletionUi.Failed
            assertTrue(again.networkError)
            assertFalse(again.retrying)
        }

    @Test
    fun `a retry of the closing met by the credit or by a key problem is told to the monitor, with the right panel`() =
        vmTest {
            backend.completed(
                job(
                    "lesson_completion",
                    JobStatus.Failed,
                    id = "C1",
                    lessonId = "L1",
                    errorCode = "llm_credit_exhausted",
                ),
            )
            val vm = opened()

            // Still exhausted: the same panel, announced again.
            backend.retryResult = ApiResult.Failure(ApiFailure.LlmCreditExhausted)
            vm.retryCompletionJob()
            runCurrent()
            val credit = ready(vm).completion as CompletionUi.Failed
            assertEquals(1, credit.creditAgain)
            assertNull(credit.retryNotice)
            assertFalse(credit.networkError)

            // The key was replaced and is refused: a new notice, not a network error.
            backend.retryResult = ApiResult.Failure(ApiFailure.LlmKeyRejected)
            vm.retryCompletionJob()
            runCurrent()
            val rejected = ready(vm).completion as CompletionUi.Failed
            assertEquals(LlmKeyVariant.Rejected, rejected.retryNotice?.variant)
            assertFalse(rejected.networkError)

            backend.retryResult = ApiResult.Failure(ApiFailure.LlmKeyMissing)
            vm.retryCompletionJob()
            runCurrent()
            val missing = ready(vm).completion as CompletionUi.Failed
            assertEquals(LlmKeyVariant.Missing, missing.retryNotice?.variant)
            assertTrue(missing.retryNotice!!.id > rejected.retryNotice!!.id)
            assertFalse(missing.networkError)

            assertEquals(listOf(LlmKeyVariant.Credit, LlmKeyVariant.Rejected, LlmKeyVariant.Missing), keys.observed)
        }

    @Test
    fun `leaving the lesson asks the topic screen for the right tab and the Tutor gets the lesson`() =
        vmTest {
            val vm = opened()
            vm.requestLessonsTab()
            assertEquals(TopicTabNames.LESSONS, inbox.take("T1"))
            vm.requestProgressTab()
            assertEquals(TopicTabNames.PROGRESS, inbox.take("T1"))
            assertNull(inbox.take("T1"))
            val ask = vm.askRequest("Non ho capito il move")!!
            assertEquals("L1", ask.lessonId)
            assertEquals("T1", ask.topicId)
            assertEquals(2, ask.lessonNumber)
            assertEquals("Non ho capito il move", ask.prompt)
        }

    @Test
    fun `the saved state keeps the reading position for the lessons list`() =
        vmTest {
            val vm = opened()
            reveal(vm)
            vm.flushState()
            val saved = LessonSavedState.decode(store.stored)
            assertEquals(1, saved.sectionNumber)
            assertEquals(1, saved.sectionTotal)
        }

    @Test
    fun `nothing is sent for a lesson that is already read only`() =
        vmTest {
            backend.completed(job("lesson_completion", JobStatus.Succeeded, id = "C1", lessonId = "L1"))
            val vm = opened()
            vm.updateExercise("q1") {
                ExerciseState(
                    selection = listOf("a"),
                    attempts = 1,
                    checked = true,
                    outcome = Outcome.Correct,
                    firstOutcome = Outcome.Correct,
                )
            }
            runCurrent()
            assertTrue(backend.requests.isEmpty())
        }

    private fun queued(
        block: String,
        type: String,
        answer: kotlinx.serialization.json.JsonObject,
        outcome: String? = null,
    ) = com.teachme.core.model
        .QueuedAttempt("L1", block, type, 1, answer, outcome, 10L)

    @Test
    fun `the lesson is not closed on the backend while answers are still queued`() =
        vmTest {
            val first = opened()
            finishLesson(first)
            backend.complete = ApiResult.Failure(NETWORK_DOWN)
            first.continueLesson()
            runCurrent()
            first.flushState()
            val completes = backend.completeCalls
            // A late answer is still in the queue and the network is down again when the lesson is reopened.
            queue.enqueue(queued("late", "quiz_single", buildJsonObject { put("option", "a") }, "correct"))
            backend.failures += NETWORK_DOWN
            backend.complete = ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
            val second = opened()
            runCurrent()
            assertEquals(completes, backend.completeCalls)
            assertEquals(CompletionUi.NeedsSend(), ready(second).completion)
            assertTrue(queue.size > 0)
        }

    @Test
    fun `the retry keeps going while the network is down`() =
        vmTest {
            repeat(3) { backend.failures += NETWORK_DOWN }
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            assertEquals(1, backend.requests.size)
            repeat(3) {
                advanceTimeBy(clock.retryMs + 1)
                runCurrent()
            }
            assertEquals(4, backend.requests.size)
            assertEquals(0, queue.size)
        }

    @Test
    fun `a refused token keeps the queue, stops retrying and says so`() =
        vmTest {
            backend.failures += ApiFailure.Unauthorized
            val vm = opened()
            reveal(vm)
            answerQuiz(vm)
            runCurrent()
            assertTrue(ready(vm).authError)
            assertEquals(1, queue.size)
            advanceTimeBy(clock.retryMs * 3)
            runCurrent()
            assertEquals(1, backend.requests.size)
            vm.retryQueueNow()
            runCurrent()
            assertFalse(ready(vm).authError)
            assertEquals(0, queue.size)
        }

    @Test
    fun `queues of lessons that are not open are sent at app start`() =
        vmTest {
            queue.enqueue(queued("q1", "quiz_single", buildJsonObject { put("option", "a") }, "correct"))
            queue.enqueue(
                queued("q1", "quiz_single", buildJsonObject { put("option", "a") }, "correct").copy(lessonId = "L9"),
            )
            AttemptSyncer(
                backend,
                queue,
                com.teachme.core.model
                    .ManualUserScopes(),
            ).flushAll()
            assertEquals(0, queue.size)
            assertEquals(2, backend.requests.size)
            // A refused token stops the sweep and keeps what is left.
            queue.enqueue(queued("q2", "quiz_single", buildJsonObject { put("option", "a") }, "correct"))
            queue.enqueue(
                queued("q2", "quiz_single", buildJsonObject { put("option", "a") }, "correct").copy(lessonId = "L9"),
            )
            backend.failures += ApiFailure.Unauthorized
            AttemptSyncer(
                backend,
                queue,
                com.teachme.core.model
                    .ManualUserScopes(),
            ).flushAll()
            assertEquals(2, queue.size)
        }
}
