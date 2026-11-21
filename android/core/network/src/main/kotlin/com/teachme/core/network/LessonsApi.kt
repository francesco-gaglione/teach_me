package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.Attempt
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonInProgress
import com.teachme.core.model.LessonView
import com.teachme.core.model.Page
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.TopicJob
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.client.request.post
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.path
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/lessons.md` section 3 and `jobs.md` section 3. One call per endpoint. */
interface LessonsApi {
    suspend fun startGeneration(topicId: String): ApiResult<TopicJob>

    suspend fun listLessons(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<Lesson>>

    suspend fun getLesson(lessonId: String): ApiResult<LessonView>

    /** `GET /lessons?status=in_progress` (contract v1.8). */
    suspend fun listInProgress(limit: Int? = null): ApiResult<Page<LessonInProgress>>

    suspend fun submitAttempts(
        lessonId: String,
        attempts: List<QueuedAttempt>,
        timeoutMs: Long? = null,
    ): ApiResult<List<Attempt>>

    suspend fun completeLesson(lessonId: String): ApiResult<TopicJob>

    suspend fun putHighlights(
        lessonId: String,
        highlights: List<HighlightRecord>,
    ): ApiResult<List<HighlightRecord>>

    suspend fun putStrokes(
        lessonId: String,
        strokes: List<StrokeRecord>,
    ): ApiResult<List<StrokeRecord>>

    suspend fun listJobs(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<TopicJob>>

    suspend fun listGlossary(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<GlossaryTerm>>

    suspend fun listRecords(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<LearningRecord>>
}

@Singleton
class KtorLessonsApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : LessonsApi {
        override suspend fun startGeneration(topicId: String): ApiResult<TopicJob> =
            apiCall({
                client.post {
                    url { path("api", "v1", "topics", topicId, "lessons") }
                    contentType(ContentType.Application.Json)
                    setBody(emptyMap<String, String>())
                }
            }) { it.body<JobDto>().toModel() }

        override suspend fun listLessons(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Lesson>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "lessons") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<LessonDto>>().toModel { dto -> dto.toModel() } }

        override suspend fun listInProgress(limit: Int?): ApiResult<Page<LessonInProgress>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "lessons") }
                    parameter("status", "in_progress")
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<LessonInProgressDto>>().toModel { dto -> dto.toModel() } }

        override suspend fun getLesson(lessonId: String): ApiResult<LessonView> =
            apiCall({ client.get { url { path("api", "v1", "lessons", lessonId) } } }) {
                it.body<LessonViewDto>().toModel()
            }

        override suspend fun submitAttempts(
            lessonId: String,
            attempts: List<QueuedAttempt>,
            timeoutMs: Long?,
        ): ApiResult<List<Attempt>> =
            apiCall({
                client.post {
                    url { path("api", "v1", "lessons", lessonId, "attempts") }
                    contentType(ContentType.Application.Json)
                    setBody(SubmitAttemptsRequest(attempts.map { it.toRequest() }))
                    timeoutMs?.let { ms -> timeout { requestTimeoutMillis = ms } }
                }
            }) { response -> response.body<SubmitAttemptsResponse>().attempts.map { it.toModel() } }

        override suspend fun completeLesson(lessonId: String): ApiResult<TopicJob> =
            apiCall({ client.post { url { path("api", "v1", "lessons", lessonId, "complete") } } }) {
                it.body<JobDto>().toModel()
            }

        override suspend fun putHighlights(
            lessonId: String,
            highlights: List<HighlightRecord>,
        ): ApiResult<List<HighlightRecord>> =
            apiCall({
                client.put {
                    url { path("api", "v1", "lessons", lessonId, "highlights") }
                    contentType(ContentType.Application.Json)
                    setBody(PutHighlightsRequest(highlights.map { it.toDto() }))
                }
            }) { response -> response.body<PutHighlightsResponse>().highlights.map { it.toModel() } }

        override suspend fun putStrokes(
            lessonId: String,
            strokes: List<StrokeRecord>,
        ): ApiResult<List<StrokeRecord>> =
            apiCall({
                client.put {
                    url { path("api", "v1", "lessons", lessonId, "strokes") }
                    contentType(ContentType.Application.Json)
                    setBody(PutStrokesRequest(strokes))
                }
            }) { response -> response.body<PutStrokesResponse>().strokes }

        override suspend fun listJobs(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<TopicJob>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "jobs") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<JobDto>>().toModel { dto -> dto.toModel() } }

        override suspend fun listGlossary(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<GlossaryTerm>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "glossary") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<GlossaryTermDto>>().toModel { dto -> dto.toModel() } }

        override suspend fun listRecords(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<LearningRecord>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "learning-records") }
                    parameter("status", "all")
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<LearningRecordDto>>().toModel { dto -> dto.toModel() } }
    }
