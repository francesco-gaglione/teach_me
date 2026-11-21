package com.teachme.sdui.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Why a block cannot be drawn. The user sees the same neutral row for all of them. */
enum class UnavailableReason {
    /** `type` is not in the catalog this app knows (newer catalog or invented). */
    UnknownType,

    /** The block has a known `type` but the JSON does not fit it (missing or wrongly typed field). */
    Malformed,

    /** Well formed but empty or inconsistent (blank text, `correct` that matches no option, unknown key...). */
    Invalid,

    /** A `missing_component` nested in a fallback, or without any drawable fallback block. */
    NoFallback,
}

/**
 * A block of the catalog v1 (docs/contracts/sdui.md section 4). The document is a flat list of them.
 * [type] is the wire name and the key of the renderer registry.
 */
sealed interface Block {
    val id: String
    val type: String
    val lang: String?
}

/** Blocks the learner answers. [required] gates the "Continue" button of the section. */
sealed interface Exercise : Block {
    val required: Boolean
}

@Serializable
@SerialName("heading")
data class Heading(
    override val id: String,
    override val lang: String? = null,
    val level: Int,
    val text: String,
) : Block {
    override val type get() = "heading"
}

@Serializable
@SerialName("paragraph")
data class Paragraph(
    override val id: String,
    override val lang: String? = null,
    val text: String,
) : Block {
    override val type get() = "paragraph"
}

@Serializable
enum class CalloutVariant {
    @SerialName("info")
    Info,

    @SerialName("tip")
    Tip,

    @SerialName("warning")
    Warning,
}

@Serializable
@SerialName("callout")
data class Callout(
    override val id: String,
    override val lang: String? = null,
    val variant: CalloutVariant,
    val title: String? = null,
    val text: String,
) : Block {
    override val type get() = "callout"
}

@Serializable
@SerialName("code")
data class CodeBlock(
    override val id: String,
    override val lang: String? = null,
    val language: String = "text",
    val code: String,
    val caption: String? = null,
    @SerialName("highlight_lines") val highlightLines: List<Int> = emptyList(),
) : Block {
    override val type get() = "code"
}

@Serializable
@SerialName("glossary_term")
data class GlossaryTermBlock(
    override val id: String,
    override val lang: String? = null,
    val key: String,
) : Block {
    override val type get() = "glossary_term"
}

@Serializable
data class ChoiceOption(
    val id: String,
    val text: String,
    val feedback: String = "",
)

/** The code an exercise asks to read (docs/contracts/sdui.md section 4.1). No caption, no highlighted lines. */
data class ExerciseCode(
    val language: String,
    val code: String,
)

@Serializable
@SerialName("quiz_single")
data class QuizSingle(
    override val id: String,
    override val lang: String? = null,
    @Serializable(with = ExerciseCodeSerializer::class) val code: ExerciseCode? = null,
    val question: String,
    val options: List<ChoiceOption>,
    val correct: String,
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "quiz_single"
}

@Serializable
@SerialName("quiz_multi")
data class QuizMulti(
    override val id: String,
    override val lang: String? = null,
    @Serializable(with = ExerciseCodeSerializer::class) val code: ExerciseCode? = null,
    val question: String,
    val options: List<ChoiceOption>,
    val correct: List<String>,
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "quiz_multi"
}

@Serializable
data class Blank(
    val id: String,
    val answers: List<String>,
    @SerialName("case_sensitive") val caseSensitive: Boolean = false,
)

@Serializable
data class OutcomeFeedback(
    val correct: String = "",
    val incorrect: String = "",
)

@Serializable
@SerialName("fill_blank")
data class FillBlank(
    override val id: String,
    override val lang: String? = null,
    val text: String,
    val blanks: List<Blank>,
    val feedback: OutcomeFeedback = OutcomeFeedback(),
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "fill_blank"
}

@Serializable
data class OrderStep(
    val id: String,
    val text: String,
)

/** [steps] are in the CORRECT order; the app shuffles them (see `OrderShuffle`). */
@Serializable
@SerialName("order_steps")
data class OrderSteps(
    override val id: String,
    override val lang: String? = null,
    val prompt: String,
    val steps: List<OrderStep>,
    val feedback: OutcomeFeedback = OutcomeFeedback(),
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "order_steps"
}

@Serializable
@SerialName("flashcard")
data class Flashcard(
    override val id: String,
    override val lang: String? = null,
    @Serializable(with = ExerciseCodeSerializer::class) val code: ExerciseCode? = null,
    val front: String,
    val back: String,
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "flashcard"
}

/** The backend-only `rubric` is accepted in the JSON and deliberately not kept: the app never shows it. */
@Serializable
@SerialName("open_answer")
data class OpenAnswer(
    override val id: String,
    override val lang: String? = null,
    @Serializable(with = ExerciseCodeSerializer::class) val code: ExerciseCode? = null,
    val question: String,
    @SerialName("max_chars") val maxChars: Int = DEFAULT_MAX_CHARS,
    override val required: Boolean = true,
) : Exercise {
    override val type get() = "open_answer"

    companion object {
        const val DEFAULT_MAX_CHARS = 1000
    }
}

@Serializable
data class ChecklistItem(
    val id: String,
    val text: String,
)

@Serializable
@SerialName("checklist")
data class Checklist(
    override val id: String,
    override val lang: String? = null,
    val title: String? = null,
    val items: List<ChecklistItem>,
) : Block {
    override val type get() = "checklist"
}

@Serializable
@SerialName("primary_source")
data class PrimarySource(
    override val id: String,
    override val lang: String? = null,
    val source: String,
    val why: String,
) : Block {
    override val type get() = "primary_source"
}

@Serializable
@SerialName("lesson_link")
data class LessonLink(
    override val id: String,
    override val lang: String? = null,
    @SerialName("lesson_id") val lessonId: String,
    val label: String,
) : Block {
    override val type get() = "lesson_link"
}

@Serializable
@SerialName("ask_agent")
data class AskAgent(
    override val id: String,
    override val lang: String? = null,
    val label: String? = null,
    @SerialName("prompt_hint") val promptHint: String,
) : Block {
    override val type get() = "ask_agent"
}

/** A node of a [Diagram]. Fields default to blank so a malformed node degrades to the text fallback, not to a neutral row. */
@Serializable
data class DiagramNode(
    val id: String = "",
    val label: String = "",
    val detail: String? = null,
)

@Serializable
data class DiagramEdge(
    val from: String = "",
    val to: String = "",
    val label: String? = null,
)

/**
 * A figure described as nodes and edges (docs/contracts/sdui.md section 10). [layout] stays a raw string so an
 * unknown layout still parses and falls back to the text version; [validateDiagram] decides what can be drawn.
 * Plain text everywhere except [caption] (inline markdown). [alt] is mandatory: without it the block is not
 * renderable (see `renderabilityProblem`).
 */
@Serializable
@SerialName("diagram")
data class Diagram(
    override val id: String,
    override val lang: String? = null,
    val layout: String,
    val title: String? = null,
    val nodes: List<DiagramNode> = emptyList(),
    val edges: List<DiagramEdge> = emptyList(),
    val caption: String = "",
    val alt: String,
) : Block {
    override val type get() = "diagram"
}

/**
 * The generator asks for a component the catalog lacks. The learner never sees [wanted] or [purpose]
 * (they feed a wish list in debug logs); the [fallback] blocks are drawn as if they were in the flow.
 */
data class MissingComponent(
    override val id: String,
    override val lang: String? = null,
    val wanted: String,
    val purpose: String,
    val fallback: List<Block>,
) : Block {
    override val type get() = "missing_component"
}

/** A block the app cannot draw: unknown `type`, malformed or inconsistent. Drawn as a neutral row. */
data class Unavailable(
    override val id: String,
    val rawType: String?,
    val reason: UnavailableReason,
    val detail: String? = null,
) : Block {
    override val type get() = rawType ?: "unknown"
    override val lang: String? get() = null
}
