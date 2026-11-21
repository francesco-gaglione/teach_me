package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessage
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatPage
import com.teachme.core.model.ChatRole
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.JobError
import com.teachme.core.model.JobStatus
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonCompletion
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.MessageError
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.Page
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.SentMessages
import com.teachme.core.model.TopicJob
import com.teachme.core.network.ChatApi
import com.teachme.core.network.LessonsApi
import com.teachme.core.network.ResourcesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.Instant

private val failedAt = Instant.ofEpochSecond(1_000)
private val finished = Instant.ofEpochSecond(1_050)

private class RecordingKeys : LlmKeyMonitor by NoLlmKeyMonitor {
    val failures = mutableListOf<Pair<LlmKeyVariant, Instant>>()
    val worked = mutableListOf<Instant>()
    override val status: StateFlow<LlmKeyStatus> = MutableStateFlow(LlmKeyStatus())

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: Instant,
    ) {
        failures += variant to at
    }

    override fun noteWorked(at: Instant) {
        worked += at
    }
}

private fun job(
    status: JobStatus,
    code: String? = null,
    kind: String = "lesson_completion",
    finishedAt: Instant? = finished,
) = TopicJob(
    "j1",
    "t1",
    kind,
    status,
    null,
    code?.let { JobError(it, "x") },
    null,
    1,
    failedAt,
    null,
    finishedAt,
)

private fun message(
    status: ChatMessageStatus,
    code: String? = null,
    role: ChatRole = ChatRole.Assistant,
) = ChatMessage(
    "m1",
    "t1",
    role,
    status,
    emptyList(),
    code?.let { MessageError(it, "x") },
    failedAt,
    finished,
)

private val lesson = Lesson("L1", "t1", 1, "T", "O", 5, LessonStatus.Completed, 1, 1, failedAt, finished)

private class FakeLessonsApi(
    var view: ApiResult<LessonView>,
    var jobs: List<TopicJob> = emptyList(),
) : LessonsApi {
    override suspend fun getLesson(lessonId: String) = view

    var inProgress: ApiResult<Page<com.teachme.core.model.LessonInProgress>> =
        ApiResult.Success(
            Page(emptyList(), null),
        )
    var inProgressLimit: Int? = null

    override suspend fun listInProgress(limit: Int?): ApiResult<Page<com.teachme.core.model.LessonInProgress>> {
        inProgressLimit = limit
        return inProgress
    }

    override suspend fun listJobs(
        topicId: String,
        cursor: String?,
        limit: Int?,
    ): ApiResult<Page<TopicJob>> = ApiResult.Success(Page(jobs, null))

    override suspend fun startGeneration(topicId: String) = error("not used")

    override suspend fun listLessons(
        topicId: String,
        cursor: String?,
        limit: Int?,
    ) = error("not used")

    override suspend fun submitAttempts(
        lessonId: String,
        attempts: List<com.teachme.core.model.QueuedAttempt>,
        timeoutMs: Long?,
    ) = error("not used")

    override suspend fun completeLesson(lessonId: String) = error("not used")

    override suspend fun putHighlights(
        lessonId: String,
        highlights: List<com.teachme.core.model.HighlightRecord>,
    ) = error("not used")

    override suspend fun putStrokes(
        lessonId: String,
        strokes: List<com.teachme.core.model.StrokeRecord>,
    ) = error("not used")

    override suspend fun listGlossary(
        topicId: String,
        cursor: String?,
        limit: Int?,
    ) = error("not used")

    override suspend fun listRecords(
        topicId: String,
        cursor: String?,
        limit: Int?,
    ) = error("not used")
}

private class FakeResourcesApi(
    var view: ResourcesView,
) : ResourcesApi {
    override suspend fun getResources(topicId: String) = ApiResult.Success(view)

    override suspend fun startResearch(topicId: String) = error("not used")

    override suspend fun retryJob(
        topicId: String,
        jobId: String,
    ) = error("not used")
}

private class FakeChatApi(
    var page: ChatPage = ChatPage(emptyList(), null, null),
    var signals: List<ChatStreamSignal> = emptyList(),
) : ChatApi {
    override suspend fun listMessages(
        topicId: String,
        cursor: String?,
        limit: Int?,
    ) = ApiResult.Success(page)

    override suspend fun sendMessage(
        topicId: String,
        text: String,
        lessonId: String?,
    ): ApiResult<SentMessages> = error("not used")

    override suspend fun retryMessage(
        topicId: String,
        messageId: String,
    ): ApiResult<ChatMessage> = error("not used")

    override fun openEvents(
        topicId: String,
        lastEventId: Long?,
    ): Flow<ChatStreamSignal> = flowOf(*signals.toTypedArray())
}

/**
 * Every job and message that enters the app through a repository says what it knows about the key, with its own date:
 * the lesson screen's polling, the lists reloaded after the stream was closed, and the stream itself.
 */
class KeyFailureObservationTest {
    private val keys = RecordingKeys()

    private fun lessons(api: FakeLessonsApi) = DefaultLessonRepository(api, FakeResourcesApi(emptyView()), keys)

    private fun emptyView(job: TopicJob? = null) = ResourcesView(emptyList(), emptyList(), false, null, job)

    private fun lessonView(job: TopicJob?) =
        LessonView(lesson, "{}", emptyList(), LessonCompletion(job, emptyList(), emptyList(), 0))

    @Test
    fun `last in progress asks for one lesson and returns the first or null`() =
        runBlocking<Unit> {
            val api = FakeLessonsApi(ApiResult.Success(lessonView(null)))
            assertEquals(ApiResult.Success(null), lessons(api).lastInProgress())
            assertEquals(1, api.inProgressLimit)
            val item =
                com.teachme.core.model
                    .LessonInProgress(lesson, "Rust", null)
            api.inProgress = ApiResult.Success(Page(listOf(item), null))
            assertEquals(item, (lessons(api).lastInProgress() as ApiResult.Success).value)
        }

    @Test
    fun `the closing job read by the lesson polling reports a key failure with its finish date`() =
        runBlocking<Unit> {
            val api = FakeLessonsApi(ApiResult.Success(lessonView(job(JobStatus.Failed, "llm_key_rejected"))))
            lessons(api).getLesson("L1")
            api.view = ApiResult.Success(lessonView(job(JobStatus.Failed, "llm_credit_exhausted")))
            lessons(api).getLesson("L1")
            api.view = ApiResult.Success(lessonView(job(JobStatus.Failed, "llm_key_missing", finishedAt = null)))
            lessons(api).getLesson("L1")

            // The finish date when there is one, otherwise the closest the job has (here its creation).
            assertEquals(
                listOf(
                    LlmKeyVariant.Rejected to finished,
                    LlmKeyVariant.Credit to finished,
                    LlmKeyVariant.Missing to failedAt,
                ),
                keys.failures,
            )
        }

    @Test
    fun `jobs found by a reload after the stream was closed report their failures, and other failures say nothing`() =
        runBlocking<Unit> {
            val api =
                FakeLessonsApi(
                    ApiResult.Success(lessonView(null)),
                    jobs =
                        listOf(
                            job(JobStatus.Failed, "llm_key_rejected", kind = "research"),
                            job(JobStatus.Failed, "invalid_output"),
                            job(JobStatus.Running),
                        ),
                )
            lessons(api).listJobs("t1")
            assertEquals(listOf(LlmKeyVariant.Rejected to finished), keys.failures)

            val resources =
                DefaultResourceRepository(FakeResourcesApi(emptyView(job(JobStatus.Failed, "llm_key_missing"))), keys)
            resources.getResources("t1")
            assertEquals(LlmKeyVariant.Missing to finished, keys.failures.last())
        }

    @Test
    fun `a finished LLM job is a proof the key works, as of its finish date`() =
        runBlocking<Unit> {
            val api = FakeLessonsApi(ApiResult.Success(lessonView(job(JobStatus.Succeeded))))
            lessons(api).getLesson("L1")
            api.jobs = listOf(job(JobStatus.Succeeded, kind = "document_import"))
            lessons(api).listJobs("t1")
            assertEquals(listOf(finished), keys.worked)
        }

    @Test
    fun `messages from a page and events from the stream report with the message date`() =
        runBlocking<Unit> {
            val failed = message(ChatMessageStatus.Failed, "llm_key_rejected")
            val api =
                FakeChatApi(
                    ChatPage(
                        listOf(
                            failed,
                            message(ChatMessageStatus.Complete),
                            message(ChatMessageStatus.Complete, role = ChatRole.User),
                        ),
                        null,
                        null,
                    ),
                    listOf(
                        ChatStreamSignal.Opened,
                        ChatStreamSignal.Event(
                            3,
                            ChatEvent.MessageFailed(message(ChatMessageStatus.Failed, "llm_credit_exhausted")),
                        ),
                        ChatStreamSignal.Event(4, ChatEvent.JobUpdated(job(JobStatus.Failed, "llm_key_missing"))),
                        ChatStreamSignal.Event(5, ChatEvent.MessageCompleted(message(ChatMessageStatus.Complete))),
                        ChatStreamSignal.Ended(null),
                    ),
                )
            val repo = DefaultChatRepository(api, keys)

            repo.listMessages("t1")
            assertEquals(listOf(LlmKeyVariant.Rejected to finished), keys.failures)
            assertEquals(listOf(finished), keys.worked)

            repo.openEvents("t1", null).toList()
            assertEquals(
                listOf(
                    LlmKeyVariant.Rejected to finished,
                    LlmKeyVariant.Credit to finished,
                    LlmKeyVariant.Missing to finished,
                ),
                keys.failures,
            )
            assertEquals(listOf(finished, finished), keys.worked)
        }
}
