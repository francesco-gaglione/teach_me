package com.teachme.core.model

import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import java.time.Instant

/** `Lesson.status` (lessons.md section 2). A value this app does not know is read as [NotStarted]. */
enum class LessonStatus(
    val wire: String,
) {
    NotStarted("not_started"),
    InProgress("in_progress"),
    Completed("completed"),
    ;

    companion object {
        fun fromWire(value: String?): LessonStatus = entries.firstOrNull { it.wire == value } ?: NotStarted
    }
}

/** A lesson of a Topic, without its document (lessons.md section 2). */
data class Lesson(
    val id: String,
    val topicId: String,
    val number: Int,
    val title: String,
    val objective: String,
    val estimatedMinutes: Int,
    val status: LessonStatus,
    val catalogVersion: Int,
    val missionVersion: Int?,
    val createdAt: Instant?,
    val completedAt: Instant?,
    /** Contract v1.8: documents followed by the plan; empty when none or from an older backend. */
    val documentSources: List<DocumentSource> = emptyList(),
)

/** One uploaded document used by a lesson, with the page range chosen by the plan (1-based, inclusive). */
data class DocumentSource(
    val documentId: String,
    val title: String,
    val pageStart: Int,
    val pageEnd: Int,
)

/** A started, unfinished lesson (`GET /lessons?status=in_progress`, contract v1.8) with its Topic title. */
data class LessonInProgress(
    val lesson: Lesson,
    val topicTitle: String,
    val lastActivityAt: Instant?,
)

/** `Attempt.outcome`. [Unknown] (a value from a newer backend) is shown like [Pending]: "Risposta registrata". */
enum class AttemptOutcome(
    val wire: String?,
) {
    Correct("correct"),
    Partial("partial"),
    Incorrect("incorrect"),
    Pending("pending"),
    Unknown(null),
    ;

    companion object {
        fun fromWire(value: String?): AttemptOutcome =
            entries.firstOrNull { it.wire != null && it.wire == value } ?: Unknown
    }
}

/** The Grader's verdict on an `open_answer` (lessons.md section 2). `misconception` is never shown to the learner. */
data class AttemptGrade(
    val verdict: AttemptOutcome,
    val feedback: String,
    val misconception: String?,
)

/** An attempt as the backend stored it (lessons.md section 2). */
data class Attempt(
    val id: String,
    val blockId: String,
    val blockType: String,
    val attemptNumber: Int,
    val answer: JsonObject,
    val outcome: AttemptOutcome,
    val grade: AttemptGrade?,
    val answeredAt: Instant?,
)

enum class RecordKind { Demonstrated, MisconceptionCorrected, Unknown }

enum class RecordStatus { Active, Superseded }

/** Structured learning record (lessons.md section 2). `body` and `evidence` are in second person. */
data class LearningRecord(
    val id: String,
    val number: Int,
    val kind: RecordKind,
    val title: String,
    val body: String,
    val evidence: String,
    val status: RecordStatus,
    val supersededBy: String?,
    val lessonId: String?,
    val createdAt: Instant?,
)

data class GlossaryTerm(
    val id: String,
    val key: String,
    val term: String,
    val definition: String,
    val avoid: List<String>,
    /** The lesson that last touched the term. */
    val lessonId: String?,
    val createdAt: Instant?,
    val updatedAt: Instant?,
    /** The lesson where the term was born (lessons.md v1.5); null for terms born earlier or from an older backend. */
    val firstLessonId: String? = null,
) {
    /** True when the term was revised after it was born. */
    val revised: Boolean get() = createdAt != null && updatedAt != null && updatedAt != createdAt
}

/**
 * A non-blocking quality note of the Validator (lessons.md section 2, `quality_warnings`). [code] may be one this app
 * does not know: it is still shown, with [message], which is already written for the learner (Italian).
 */
@Serializable
data class QualityWarning(
    val code: String,
    /** The block the note is about (the first one, if several), or null when it is about the whole lesson. */
    val blockId: String?,
    val message: String,
)

/** `LessonView.completion` (lessons.md section 3.1): the closing job and what it produced. */
data class LessonCompletion(
    val job: TopicJob?,
    val records: List<LearningRecord>,
    val terms: List<GlossaryTerm>,
    val reviewCardCount: Int,
)

/** `GET /lessons/{id}`: the document stays raw JSON, the SDUI renderer parses it. */
data class LessonView(
    val lesson: Lesson,
    val documentJson: String,
    val attempts: List<Attempt>,
    val completion: LessonCompletion?,
    /** The user's highlights as the backend saved them (lessons.md section 10); empty from a backend without them. */
    val highlights: List<HighlightRecord> = emptyList(),
    /** The user's handwritten notes in drawing order (lessons.md section 11); empty from a backend without them. */
    val strokes: List<StrokeRecord> = emptyList(),
    /** Validator notes (lessons.md v1.7); empty from an older backend. */
    val qualityWarnings: List<QualityWarning> = emptyList(),
    /** Content language of the lesson's Topic (lessons.md v1.9); Italian from an older backend. */
    val language: TopicLanguage = TopicLanguage.It,
)

/**
 * An attempt waiting to be sent (lessons.md section 5), persisted in the local queue. The pair
 * ([lessonId], [blockId], [attemptNumber]) is its identity: queuing the same pair again replaces it.
 */
@Serializable
data class QueuedAttempt(
    val lessonId: String,
    val blockId: String,
    val blockType: String,
    val attemptNumber: Int,
    val answer: JsonObject,
    /** Local verdict of closed exercises (`correct` | `partial` | `incorrect`); null for flashcards and open answers. */
    val outcome: String? = null,
    /** Epoch milliseconds of the moment the learner answered. */
    val answeredAtMs: Long,
) {
    val isOpenAnswer: Boolean get() = blockType == BLOCK_TYPE_OPEN_ANSWER

    fun sameAttempt(other: QueuedAttempt): Boolean =
        lessonId == other.lessonId && blockId == other.blockId && attemptNumber == other.attemptNumber
}

const val BLOCK_TYPE_OPEN_ANSWER = "open_answer"

/** Persistent queue of attempts not yet registered by the backend. Survives restarts. */
interface AttemptQueue {
    /** Adds [attempt], replacing one of the same pair. */
    suspend fun enqueue(attempt: QueuedAttempt)

    /** Attempts of [lessonId] in the order they were queued. */
    suspend fun pending(lessonId: String): List<QueuedAttempt>

    /** Lessons with something in the queue. */
    suspend fun lessonsWithPending(): List<String>

    /** Every attempt waiting, whatever the lesson. */
    suspend fun pendingTotal(): Int

    /** Removes exactly these attempts (by pair). */
    suspend fun remove(attempts: List<QueuedAttempt>)

    suspend fun clear(lessonId: String)

    /** How many attempts of [lessonId] are waiting; emits on every change. */
    fun count(lessonId: String): Flow<Int>
}

/** Lessons, attempts, learning records and glossary of a Topic (docs/contracts/lessons.md). No cache: every call goes out. */
interface LessonRepository {
    /** `POST /topics/{id}/lessons`: starts a generation; a 409 is [ApiFailure.Conflict]. */
    suspend fun startGeneration(topicId: String): ApiResult<TopicJob>

    /** One page of lessons, `number` descending. */
    suspend fun listLessons(
        topicId: String,
        cursor: String? = null,
    ): ApiResult<Page<Lesson>>

    suspend fun getLesson(lessonId: String): ApiResult<LessonView>

    /** `GET /lessons?status=in_progress&limit=1`: the most recently worked on lesson, or null when there is none. */
    suspend fun lastInProgress(): ApiResult<LessonInProgress?>

    /**
     * `POST /lessons/{id}/attempts`. The result lists the attempts as saved, in the order sent. Waits for the Grader when
     * a batch has an `open_answer`: the call gives up after [OPEN_ANSWER_CLIENT_TIMEOUT_MS].
     */
    suspend fun submitAttempts(
        lessonId: String,
        attempts: List<QueuedAttempt>,
    ): ApiResult<List<Attempt>>

    /** `POST /lessons/{id}/complete`; a 409 is [ApiFailure.Conflict] (already completed). */
    suspend fun completeLesson(lessonId: String): ApiResult<TopicJob>

    /**
     * `PUT /lessons/{id}/highlights`: replaces the whole set (lessons.md section 10). A rejected entry is
     * [ApiFailure.Validation] with `highlights[i]` as the field of its details.
     */
    suspend fun putHighlights(
        lessonId: String,
        highlights: List<HighlightRecord>,
    ): ApiResult<List<HighlightRecord>>

    /**
     * `PUT /lessons/{id}/strokes`: replaces the whole set, in drawing order (lessons.md section 11). A rejected stroke
     * is [ApiFailure.Validation] with `strokes[i]` as the field of its details; `413` is [ApiFailure.Unexpected] with
     * that `httpStatus`.
     */
    suspend fun putStrokes(
        lessonId: String,
        strokes: List<StrokeRecord>,
    ): ApiResult<List<StrokeRecord>>

    /** `GET /topics/{id}/jobs`, newest first: the current job of a kind is the first with that kind. */
    suspend fun listJobs(topicId: String): ApiResult<List<TopicJob>>

    /** `POST /topics/{id}/jobs/{job_id}/retry`. */
    suspend fun retryJob(
        topicId: String,
        jobId: String,
    ): ApiResult<TopicJob>

    /** Every page of the glossary, alphabetical. */
    suspend fun listGlossary(topicId: String): ApiResult<List<GlossaryTerm>>

    /** Every page of the learning records (`status=all`), `number` descending. */
    suspend fun listRecords(topicId: String): ApiResult<List<LearningRecord>>
}

/** The client gives up on an attempt batch with an `open_answer` after this long (lesson.md section 8.1). */
const val OPEN_ANSWER_CLIENT_TIMEOUT_MS = 25_000L

/**
 * Hand-over between screens of the same Topic: "Torna all'argomento" from the lesson asks the Topic screen to show a
 * given tab (the wire name of the tab), once. In memory only.
 */
interface TopicTabInbox {
    fun post(
        topicId: String,
        tab: String,
    )

    /** The tab requested for [topicId], or null; reading consumes it. */
    fun take(topicId: String): String?
}

/** Tabs a screen can ask the Topic screen to show (the Topic screen maps them to its own tab type). */
object TopicTabNames {
    const val LESSONS = "lessons"
    const val CHAT = "chat"
    const val PROGRESS = "progress"
}

class InMemoryTopicTabInbox : TopicTabInbox {
    private val requests = HashMap<String, String>()

    @Synchronized
    override fun post(
        topicId: String,
        tab: String,
    ) {
        requests[topicId] = tab
    }

    @Synchronized
    override fun take(topicId: String): String? = requests.remove(topicId)
}
