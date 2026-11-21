package com.teachme.feature.lesson

import com.teachme.core.model.DocumentSource
import com.teachme.core.model.JobStatus
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicState

/** The phases of lesson generation in words (topic.md F3.C). */
enum class GenerationPhase {
    Planning,

    /** "Leggo le fonti": only when the plan follows documents or web sources (lessons.md section 4, v1.8). */
    Reading,
    Writing,
    Checking,
    Saving,
    ;

    companion object {
        fun fromStep(step: String?): GenerationPhase? =
            when (step) {
                "planning" -> Planning
                "reading" -> Reading
                "writing" -> Writing
                "checking" -> Checking
                "saving" -> Saving
                else -> null
            }
    }
}

enum class PhaseState { Done, Current, Todo }

/**
 * `writing` with `done` above 1 (the message is localized, so it is never matched): the Generator is fixing what the Validator found
 * (lessons.md section 4). The screen then says "Correggo la lezione" and shows "Controllo" as still to do.
 */
internal fun TopicJob.isRepair(): Boolean = progress?.step == "writing" && (progress?.done ?: 0) > 1

/**
 * The phases with the state each one shows. [GenerationPhase.Reading] is listed only when [showReading] (the step has
 * arrived: without sources it never does). Repair turns "Scrivo" into "Correggo" and puts "Controllo" back to future.
 */
internal fun generationPhases(
    job: TopicJob?,
    completing: Boolean,
    showReading: Boolean = false,
): List<Pair<GenerationPhase, PhaseState>> {
    val phases = GenerationPhase.entries
    val current = if (completing) null else GenerationPhase.fromStep(job?.progress?.step)
    val currentIndex = current?.let(phases::indexOf)
    return phases
        .mapIndexed { i, phase ->
            phase to
                when {
                    completing -> PhaseState.Done
                    currentIndex == null -> if (i == 0) PhaseState.Current else PhaseState.Todo
                    i < currentIndex -> PhaseState.Done
                    i == currentIndex -> PhaseState.Current
                    else -> PhaseState.Todo
                }
        }.filter { (phase, _) -> phase != GenerationPhase.Reading || showReading }
}

/** One "Dal tuo PDF" line of a lesson row (topic.md, Lista lezioni). [title] is null with a single document. */
data class PdfLine(
    val title: String?,
    val pageStart: Int,
    val pageEnd: Int,
) {
    val singlePage: Boolean get() = pageStart == pageEnd
}

/** The lines to show (at most [MAX_PDF_LINES]) and how many documents are left out ("e altri k"). */
data class PdfLines(
    val lines: List<PdfLine>,
    val more: Int,
)

internal const val MAX_PDF_LINES = 3

/** Empty sources: no line. One document: no title. Several: one line each with the title, the first 3 and "e altri k". */
fun pdfLines(sources: List<DocumentSource>): PdfLines {
    if (sources.isEmpty()) return PdfLines(emptyList(), 0)
    val single = sources.size == 1
    return PdfLines(
        sources.take(MAX_PDF_LINES).map { PdfLine(if (single) null else it.title, it.pageStart, it.pageEnd) },
        (sources.size - MAX_PDF_LINES).coerceAtLeast(0),
    )
}

/** What the "Prossima lezione" card shows (topic.md F3.E). */
sealed interface LessonsCard {
    /** The Topic is not `ready` (F3.A). */
    data class Blocked(
        val topicState: TopicState,
        val researchFailed: Boolean,
    ) : LessonsCard

    /** F3.B: no lesson, or the last one is completed. */
    data class Free(
        val hasLessons: Boolean,
    ) : LessonsCard

    /** F3.B: a lesson is still to be finished. */
    data class Unfinished(
        val lesson: Lesson,
    ) : LessonsCard

    /** F3.C: a `lesson_generation` job is queued or running. */
    data class Preparing(
        val job: TopicJob,
    ) : LessonsCard

    /** F3.D. */
    data class Failed(
        val job: TopicJob,
    ) : LessonsCard
}

fun cardFor(
    topicState: TopicState,
    researchFailed: Boolean,
    lessons: List<Lesson>,
    job: TopicJob?,
): LessonsCard {
    if (job != null && job.isActive) return LessonsCard.Preparing(job)
    if (topicState != TopicState.Ready) return LessonsCard.Blocked(topicState, researchFailed)
    if (job != null && job.status == JobStatus.Failed) return LessonsCard.Failed(job)
    val last = lessons.firstOrNull()
    return if (last != null &&
        last.status != LessonStatus.Completed
    ) {
        LessonsCard.Unfinished(last)
    } else {
        LessonsCard.Free(last != null)
    }
}

/**
 * Whether [this] update of a lesson job may replace [previous]: another job wins if it is newer; the same job never goes
 * back (a replayed event or a late HTTP answer), except the retry (failed goes back to queued).
 */
internal fun TopicJob.supersedes(previous: TopicJob): Boolean =
    when {
        id != previous.id -> createdAt >= previous.createdAt
        previous.status == JobStatus.Failed && status == JobStatus.Queued -> true
        else -> rank() >= previous.rank()
    }

private fun TopicJob.rank(): Int =
    when (status) {
        JobStatus.Queued -> 0
        JobStatus.Succeeded, JobStatus.Failed -> 2
        else -> 1
    }

/** First letter of a glossary term, without accent, upper case; digits and symbols go under "#" (progress.md 4.1). */
internal fun glossaryInitial(term: String): String {
    val first =
        java.text.Normalizer
            .normalize(term.trim(), java.text.Normalizer.Form.NFD)
            .firstOrNull { it.isLetterOrDigit() }
            ?: return "#"
    return if (first.isLetter()) first.uppercaseChar().toString() else "#"
}

private fun String.fold(): String =
    java.text.Normalizer
        .normalize(this, java.text.Normalizer.Form.NFD)
        .filter { !Character.getType(it).let { t -> t == Character.NON_SPACING_MARK.toInt() } }
        .lowercase()

/** Case- and accent-insensitive search over term, key and definition (progress.md 4.1). */
internal fun matchesQuery(
    term: com.teachme.core.model.GlossaryTerm,
    query: String,
): Boolean {
    val q = query.trim().fold()
    if (q.isEmpty()) return true
    return term.term.fold().contains(q) || term.key.fold().contains(q) || term.definition.fold().contains(q)
}

/** The "open lesson" button of the glossary sheet depends on the target and the action, never on the number resolving. */
internal fun shouldOfferOpenLesson(
    origin: LessonOrigin?,
    canOpen: Boolean,
): Boolean = origin != null && canOpen

/** The origin line of a glossary term sheet (progress.md section 4.2); each case says only what is known. */
sealed interface TermOriginLine {
    /** Born in lesson [first] and never revised. */
    data class Introduced(
        val first: Int,
    ) : TermOriginLine

    /** Born in lesson [first], last revised in lesson [last]. */
    data class IntroducedRevised(
        val first: Int,
        val last: Int,
    ) : TermOriginLine

    /** Origin unknown (or its number unresolvable); last touched by lesson [last]. */
    data class Revised(
        val last: Int,
    ) : TermOriginLine
}

/**
 * Picks the origin line from `first_lesson_id` and `lesson_id`. [numberOf] resolves a lesson id to its number, null when
 * unknown (lesson gone, list not loaded). A number that does not resolve drops only its half; with none, the result is
 * null: no line, never a placeholder.
 */
fun termOriginLine(
    firstLessonId: String?,
    lessonId: String?,
    numberOf: (String) -> Int?,
): TermOriginLine? {
    val first = firstLessonId?.let(numberOf)
    val last = lessonId?.let(numberOf)
    return when {
        firstLessonId != null && (lessonId == null || firstLessonId == lessonId) ->
            first?.let { TermOriginLine.Introduced(it) }
        firstLessonId != null ->
            when {
                first != null && last != null -> TermOriginLine.IntroducedRevised(first, last)
                first != null -> TermOriginLine.Introduced(first)
                last != null -> TermOriginLine.Revised(last)
                else -> null
            }
        else -> last?.let { TermOriginLine.Revised(it) }
    }
}
