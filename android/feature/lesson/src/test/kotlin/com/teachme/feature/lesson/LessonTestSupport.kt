package com.teachme.feature.lesson

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptGrade
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.HighlightStore
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonCompletion
import com.teachme.core.model.LessonDocumentSource
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LessonStateStore
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.Page
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.RecordKind
import com.teachme.core.model.RecordStatus
import com.teachme.core.model.StoredHighlights
import com.teachme.core.model.StoredStrokes
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.StrokeStore
import com.teachme.core.model.TopicJob
import com.teachme.core.model.UnreachableReason
import com.teachme.sdui.model.SduiEvent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import java.time.Instant

internal val NETWORK_DOWN = ApiFailure.Network(UnreachableReason.ConnectionRefused)

/** A small lesson: an introduction, then one section with a quiz and an open answer, both required. */
internal const val SMALL_LESSON =
    """{"catalog_version":1,"title":"Ownership","objective":"Capire il move.","estimated_minutes":5,
        "glossary":[],"sources":[],"blocks":[
        {"id":"h1","type":"heading","level":1,"text":"Ownership"},
        {"id":"s1","type":"heading","level":2,"text":"Uno"},
        {"id":"q1","type":"quiz_single","question":"?","options":[{"id":"a","text":"A","feedback":"f"},{"id":"b","text":"B","feedback":"f"}],"correct":"a"},
        {"id":"oa","type":"open_answer","question":"Perché?","max_chars":300}]}"""

internal fun lesson(
    id: String = "L1",
    status: LessonStatus = LessonStatus.InProgress,
    number: Int = 2,
    title: String = "Ownership",
) = Lesson(id, "T1", number, title, "Capire il move.", 5, status, 1, 1, Instant.EPOCH, null)

internal fun job(
    kind: String,
    status: JobStatus,
    id: String = "J1",
    lessonId: String? = null,
    step: String? = null,
    message: String? = null,
    done: Int? = null,
    errorCode: String? = null,
    result: JobResult? = null,
    createdSecond: Long = 1,
) = TopicJob(
    id = id,
    topicId = "T1",
    kind = kind,
    status = status,
    progress = step?.let { JobProgress(it, message, done, null) },
    error = errorCode?.let { JobError(it, "dettaglio tecnico") },
    result = result,
    attempt = 1,
    createdAt = Instant.ofEpochSecond(createdSecond),
    startedAt = null,
    finishedAt = null,
    lessonId = lessonId,
)

internal fun record(
    id: String = "r1",
    number: Int = 1,
    status: RecordStatus = RecordStatus.Active,
    supersededBy: String? = null,
) = LearningRecord(
    id,
    number,
    RecordKind.Demonstrated,
    "Titolo $number",
    "Corpo",
    "Prova",
    status,
    supersededBy,
    "L1",
    Instant.EPOCH,
)

internal fun term(
    key: String,
    term: String = key,
) = GlossaryTerm("id-$key", key, term, "Definizione di $term.", emptyList(), "L1", Instant.EPOCH, Instant.EPOCH)

/** The attempt as a backend would save it: the closed outcome is echoed, an open answer gets [grade]. */
internal fun saved(
    queued: QueuedAttempt,
    outcome: AttemptOutcome? = null,
    feedback: String? = null,
): Attempt {
    val result = outcome ?: queued.outcome?.let { AttemptOutcome.fromWire(it) } ?: AttemptOutcome.Pending
    return Attempt(
        id = "${queued.blockId}-${queued.attemptNumber}",
        blockId = queued.blockId,
        blockType = queued.blockType,
        attemptNumber = queued.attemptNumber,
        answer = queued.answer,
        outcome = result,
        grade = feedback?.let { AttemptGrade(result, it, null) },
        answeredAt = Instant.ofEpochMilli(queued.answeredAtMs),
    )
}

/** An in-memory [AttemptQueue]; tests that must survive a "restart" share the same instance. */
internal class MemoryQueue : AttemptQueue {
    private val items = MutableStateFlow<List<QueuedAttempt>>(emptyList())

    override suspend fun enqueue(attempt: QueuedAttempt) {
        items.value =
            items.value.let { list ->
                val i = list.indexOfFirst { it.sameAttempt(attempt) }
                if (i < 0) list + attempt else list.toMutableList().also { it[i] = attempt }
            }
    }

    override suspend fun pending(lessonId: String) = items.value.filter { it.lessonId == lessonId }

    override suspend fun lessonsWithPending() = items.value.map { it.lessonId }.distinct()

    override suspend fun pendingTotal() = items.value.size

    override suspend fun remove(attempts: List<QueuedAttempt>) {
        items.value = items.value.filter { q -> attempts.none { it.sameAttempt(q) } }
    }

    override suspend fun clear(lessonId: String) {
        items.value = items.value.filter { it.lessonId != lessonId }
    }

    override fun count(lessonId: String): Flow<Int> =
        items
            .map { l ->
                l.count {
                    it.lessonId == lessonId
                }
            }.distinctUntilChanged()

    val size: Int get() = items.value.size
}

/** A backend that keeps the first attempt of every pair (lessons.md section 5) and records what it is sent. */
internal class FakeLessons : LessonRepository {
    var view: ApiResult<LessonView> = ApiResult.Success(LessonView(lesson(), SMALL_LESSON, emptyList(), null))
    var getCalls = 0

    /** Every request to `/attempts`, in order. */
    val requests = mutableListOf<List<QueuedAttempt>>()

    /** The attempts the backend has stored, by (lesson, block, number): the first one wins. */
    val store = linkedMapOf<Triple<String, String, Int>, Attempt>()

    /** What the Grader says about an open answer; null means "could not grade": `pending`. */
    var grade: (QueuedAttempt) -> Pair<AttemptOutcome, String>? = { null }

    /** A failure for the next requests (taken one by one); an empty list means success. */
    val failures = ArrayDeque<ApiFailure>()

    var complete: ApiResult<TopicJob> =
        ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))
    var completeCalls = 0
    var retryCalls = 0
    var retryResult: ApiResult<TopicJob> =
        ApiResult.Success(job("lesson_completion", JobStatus.Queued, id = "C1", lessonId = "L1"))

    var started: ApiResult<TopicJob> = ApiResult.Success(job("lesson_generation", JobStatus.Queued, id = "G1"))
    var startCalls = 0
    var lessonPages: Map<String?, ApiResult<Page<Lesson>>> = mapOf(null to ApiResult.Success(Page(emptyList(), null)))
    val listedCursors = mutableListOf<String?>()
    var jobs: ApiResult<List<TopicJob>> = ApiResult.Success(emptyList())
    var glossary: ApiResult<List<GlossaryTerm>> = ApiResult.Success(emptyList())
    var records: ApiResult<List<LearningRecord>> = ApiResult.Success(emptyList())

    override suspend fun startGeneration(topicId: String): ApiResult<TopicJob> {
        startCalls++
        return started
    }

    override suspend fun listLessons(
        topicId: String,
        cursor: String?,
    ): ApiResult<Page<Lesson>> {
        listedCursors += cursor
        return lessonPages.getValue(cursor)
    }

    override suspend fun getLesson(lessonId: String): ApiResult<LessonView> {
        getCalls++
        return (view as? ApiResult.Success)?.let { ApiResult.Success(it.value.copy(attempts = store.values.toList())) }
            ?: view
    }

    override suspend fun lastInProgress(): ApiResult<com.teachme.core.model.LessonInProgress?> = ApiResult.Success(null)

    override suspend fun submitAttempts(
        lessonId: String,
        attempts: List<QueuedAttempt>,
    ): ApiResult<List<Attempt>> {
        requests += attempts
        failures.removeFirstOrNull()?.let { return ApiResult.Failure(it) }
        val out =
            attempts.map { q ->
                val key = Triple(lessonId, q.blockId, q.attemptNumber)
                val old = store[key]
                // First wins, except that a pending open answer is graded again (lessons.md section 5.1).
                val result =
                    when {
                        old != null && old.outcome != AttemptOutcome.Pending -> old
                        q.isOpenAnswer -> grade(q)?.let { (o, f) -> saved(q, o, f) } ?: saved(q, AttemptOutcome.Pending)
                        else -> old ?: saved(q)
                    }
                store[key] = result
                result
            }
        return ApiResult.Success(out)
    }

    /** Every `PUT /highlights`, in order. */
    val highlightRequests = mutableListOf<List<HighlightRecord>>()

    /** What the backend holds after the last accepted `PUT`. */
    var storedHighlights: List<HighlightRecord> = emptyList()

    /** A failure for the next `PUT`s (taken one by one); an empty list means success. */
    val highlightFailures = ArrayDeque<ApiFailure>()

    /** Lets a test do something while a `PUT` is out (an edit made in flight). */
    var onPutHighlights: suspend () -> Unit = {}

    override suspend fun putHighlights(
        lessonId: String,
        highlights: List<HighlightRecord>,
    ): ApiResult<List<HighlightRecord>> {
        highlightRequests += highlights
        onPutHighlights()
        highlightFailures.removeFirstOrNull()?.let { return ApiResult.Failure(it) }
        storedHighlights = highlights
        return ApiResult.Success(highlights)
    }

    /** Every `PUT /strokes`, in order. */
    val strokeRequests = mutableListOf<List<StrokeRecord>>()

    /** What the backend holds after the last accepted `PUT /strokes`. */
    var storedStrokes: List<StrokeRecord> = emptyList()

    /** A failure for the next `PUT /strokes` (taken one by one); an empty list means success. */
    val strokeFailures = ArrayDeque<ApiFailure>()

    /** Lets a test do something while a `PUT /strokes` is out (an edit made in flight). */
    var onPutStrokes: suspend () -> Unit = {}

    override suspend fun putStrokes(
        lessonId: String,
        strokes: List<StrokeRecord>,
    ): ApiResult<List<StrokeRecord>> {
        strokeRequests += strokes
        onPutStrokes()
        strokeFailures.removeFirstOrNull()?.let { return ApiResult.Failure(it) }
        storedStrokes = strokes
        return ApiResult.Success(strokes)
    }

    override suspend fun completeLesson(lessonId: String): ApiResult<TopicJob> {
        completeCalls++
        return complete
    }

    override suspend fun listJobs(topicId: String) = jobs

    override suspend fun retryJob(
        topicId: String,
        jobId: String,
    ): ApiResult<TopicJob> {
        retryCalls++
        return retryResult
    }

    override suspend fun listGlossary(topicId: String) = glossary

    override suspend fun listRecords(topicId: String) = records

    /** A lesson the backend has closed: `completion` carries [job] and what it produced. */
    fun completed(
        job: TopicJob,
        records: List<LearningRecord> = emptyList(),
        terms: List<GlossaryTerm> = emptyList(),
        cards: Int = 0,
    ) {
        view =
            ApiResult.Success(
                LessonView(
                    lesson(status = LessonStatus.Completed),
                    SMALL_LESSON,
                    emptyList(),
                    LessonCompletion(job, records, terms, cards),
                ),
            )
    }
}

internal class FakeSource(
    private val text: String?,
) : LessonDocumentSource {
    override suspend fun load(key: String): String? = text
}

/** The lesson state ([stored]) and the copies of downloaded documents ("doc/..." keys) live apart, like different keys. */
internal class FakeStore(
    var stored: String? = null,
) : LessonStateStore {
    var saves = 0
    private val documents = mutableMapOf<String, String>()

    override suspend fun load(key: String): String? = if (key.startsWith("doc/")) documents[key] else stored

    override fun save(
        key: String,
        json: String,
    ) {
        if (key.startsWith("doc/")) {
            documents[key] = json
        } else {
            stored = json
            saves++
        }
    }
}

internal class FailingStore : LessonStateStore {
    override suspend fun load(key: String): String? = throw java.io.IOException("disk")

    override fun save(
        key: String,
        json: String,
    ) = Unit
}

internal class ImmediateDispatchers : LessonDispatchers() {
    var now = 1_000L
    override val parse: kotlinx.coroutines.CoroutineDispatcher = kotlinx.coroutines.Dispatchers.Unconfined

    override fun nowMs(): Long = now
}

internal class FakeLogger : SduiEventLogger({ false }) {
    val events = mutableListOf<SduiEvent>()

    override fun record(event: SduiEvent) {
        events += event
    }
}

/** An in-memory [HighlightStore]; tests that must survive a "restart" share the same instance. */
internal class MemoryHighlightStore : HighlightStore {
    val entries = linkedMapOf<String, StoredHighlights>()
    var color: String? = null

    /** Makes the next `update` throw, like a failing disk write. */
    var failNextUpdate = false

    /** Every `update` throws an `IOException` while this is set (a full disk); nothing is changed. */
    var diskFailing = false

    override suspend fun get(lessonId: String) = entries[lessonId]

    override suspend fun update(
        lessonId: String,
        transform: (StoredHighlights?) -> StoredHighlights?,
    ) {
        if (failNextUpdate) {
            failNextUpdate = false
            error("disk full")
        }
        if (diskFailing) throw java.io.IOException("no space left on device")
        val new = transform(entries[lessonId])
        if (new == null) entries.remove(lessonId) else entries[lessonId] = new.copy(lessonId = lessonId)
    }

    override suspend fun dirtyLessons() = entries.values.filter { it.dirty }.map { it.lessonId }

    override suspend fun color() = color

    override suspend fun setColor(color: String) {
        this.color = color
    }
}

/** An in-memory [StrokeStore]; tests that must survive a "restart" share the same instance. */
internal class MemoryStrokeStore : StrokeStore {
    val entries = linkedMapOf<String, StoredStrokes>()
    var tool: String? = null
    var color: String? = null
    var width: String? = null

    /** Makes the next `update` throw, like a failing disk write. */
    var failNextUpdate = false

    /** Every `update` throws an `IOException` while this is set (a full disk); nothing is changed. */
    var diskFailing = false

    override suspend fun get(lessonId: String) = entries[lessonId]

    override suspend fun update(
        lessonId: String,
        transform: (StoredStrokes?) -> StoredStrokes?,
    ) {
        if (failNextUpdate) {
            failNextUpdate = false
            error("disk full")
        }
        if (diskFailing) throw java.io.IOException("no space left on device")
        val new = transform(entries[lessonId])
        if (new == null) entries.remove(lessonId) else entries[lessonId] = new.copy(lessonId = lessonId)
    }

    override suspend fun dirtyLessons() = entries.values.filter { it.dirty }.map { it.lessonId }

    override suspend fun penTool() = tool

    override suspend fun setPenTool(tool: String) {
        this.tool = tool
    }

    override suspend fun inkColor() = color

    override suspend fun setInkColor(color: String) {
        this.color = color
    }

    override suspend fun penWidth() = width

    override suspend fun setPenWidth(width: String) {
        this.width = width
    }
}
