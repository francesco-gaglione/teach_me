package com.teachme.sdui.render

import androidx.compose.runtime.Stable
import androidx.compose.runtime.staticCompositionLocalOf
import com.teachme.core.model.LlmKeyVariant
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.LessonDocument

/** What the lesson host (the screen) does for the blocks. The renderer never navigates by itself. */
interface SduiHost {
    /** Opens the glossary sheet of [key]. */
    fun openTerm(key: String)

    /** Opens the "Fonte" sheet of the source [key]. */
    fun openSource(key: String)

    /** Opens an https link in the browser. */
    fun openUrl(url: String)

    /** Opens the Tutor chat with the composer prefilled (not sent). */
    fun askAgent(promptHint: String)

    fun openLesson(lessonId: String)

    /** Title of the topic a linked lesson belongs to, or null when the lesson is not available. */
    fun lessonLinkTarget(lessonId: String): LessonLinkTarget?

    fun copyText(text: String)

    /** "Valuta ora": sends the open answer of [blockId] again so the Grader tries once more. */
    fun reevaluate(blockId: String) = Unit

    /** Opens Settings on the LLM key section (llm-key.md): the answer panel offers it instead of "Valuta ora". */
    fun openKeySettings(variant: LlmKeyVariant) = Unit
}

data class LessonLinkTarget(
    val topicTitle: String?,
)

/** Reads and writes what the learner did on a block. Implemented by the lesson state holder. */
interface ExerciseStates {
    fun get(blockId: String): ExerciseState

    fun update(
        blockId: String,
        transform: (ExerciseState) -> ExerciseState,
    )

    /** True for exercises of a section already passed with "Continua": read-only, no retry. */
    fun isLocked(blockId: String): Boolean
}

const val DEFAULT_CONTENT_LANGUAGE = "it"

/** Everything a block needs from its surroundings. Provided once per lesson through [LocalSdui]. */
@Stable
class SduiEnvironment(
    val document: LessonDocument,
    val host: SduiHost,
    val states: ExerciseStates,
    /** True when a hardware keyboard is attached: shortcut hints are shown. */
    val hardwareKeyboard: Boolean = false,
    val nowMs: () -> Long = System::currentTimeMillis,
    /** The lesson is `completed`: exercises without an answer are disabled (lesson.md section 7.3). */
    val readOnly: Boolean = false,
    /**
     * Why an answer that waits for its grade cannot be graded: the key is known to be missing or refused (llm-key.md 5.1),
     * or the credit was seen exhausted (9.5, [LlmKeyVariant.Credit]: the button stays "Valuta ora"). Null when unknown.
     */
    val keyProblem: LlmKeyVariant? = null,
    /**
     * Language of the Topic the lesson belongs to (ADR 0019 point c): what a block without `lang` is written in. ISO 639-1,
     * "it" until the host knows better.
     */
    val contentLanguage: String = DEFAULT_CONTENT_LANGUAGE,
) {
    /** True while keyboard focus is inside an exercise card: Ctrl+Enter then belongs to the exercise. */
    var focusInExercise: Boolean = false
}

val LocalSdui =
    staticCompositionLocalOf<SduiEnvironment> {
        error("SduiEnvironment not provided: wrap the lesson in CompositionLocalProvider(LocalSdui provides ...)")
    }
