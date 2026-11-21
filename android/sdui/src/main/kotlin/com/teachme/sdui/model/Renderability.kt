package com.teachme.sdui.model

private val PLACEHOLDER = Regex("""\{\{([a-z0-9_-]{1,40})\}\}""")

/** Ids of the `{{id}}` placeholders of a fill_blank text, in order of appearance (repeats included). */
fun placeholderIds(text: String): List<String> = PLACEHOLDER.findAll(text).map { it.groupValues[1] }.toList()

/** Splits a fill_blank text into literal chunks and placeholder ids, in order. */
sealed interface BlankSegment {
    data class Literal(
        val text: String,
    ) : BlankSegment

    data class Slot(
        val blankId: String,
    ) : BlankSegment
}

fun splitBlankText(text: String): List<BlankSegment> {
    val out = mutableListOf<BlankSegment>()
    var last = 0
    for (m in PLACEHOLDER.findAll(text)) {
        if (m.range.first > last) out += BlankSegment.Literal(text.substring(last, m.range.first))
        out += BlankSegment.Slot(m.groupValues[1])
        last = m.range.last + 1
    }
    if (last < text.length) out += BlankSegment.Literal(text.substring(last))
    return out
}

private fun String.blank() = isBlank()

private fun List<String>.hasDuplicates() = size != toSet().size

/**
 * Why [block] cannot be drawn although it parsed (docs/design/components/README.md "Contenuti inattesi"),
 * or null when it is fine. The validator on the backend normally prevents all of this; the app does not
 * trust that. [document] resolves references to the glossary and the sources.
 */
internal fun renderabilityProblem(
    block: Block,
    glossaryKeys: Set<String>,
    sourceKeys: Set<String>,
): String? =
    when (block) {
        is Heading -> if (block.level !in 1..3 || block.text.blank()) "bad heading" else null
        is Paragraph -> if (block.text.blank()) "empty text" else null
        is Callout -> if (block.text.blank()) "empty text" else null
        is CodeBlock -> if (block.code.blank()) "empty code" else null
        is GlossaryTermBlock -> if (block.key !in glossaryKeys) "unknown glossary key" else null
        is QuizSingle -> choiceProblem(block.question, block.options, listOf(block.correct), single = true)
        is QuizMulti -> choiceProblem(block.question, block.options, block.correct, single = false)
        is FillBlank -> fillBlankProblem(block)
        is OrderSteps ->
            when {
                block.prompt.blank() -> "empty prompt"
                block.steps.size < 2 -> "fewer than two steps"
                block.steps.any { it.text.blank() } -> "empty step"
                block.steps.map { it.id }.hasDuplicates() -> "duplicate step ids"
                else -> null
            }
        is Flashcard -> if (block.front.blank() || block.back.blank()) "empty card" else null
        is OpenAnswer -> if (block.question.blank()) "empty question" else null
        is Checklist ->
            when {
                block.items.isEmpty() -> "no items"
                block.items.any { it.text.blank() } -> "empty item"
                block.items.map { it.id }.hasDuplicates() -> "duplicate item ids"
                else -> null
            }
        is PrimarySource ->
            when {
                block.source !in sourceKeys -> "unknown source key"
                block.why.blank() -> "empty why"
                else -> null
            }
        is LessonLink -> if (block.lessonId.blank() || block.label.blank()) "empty link" else null
        is AskAgent -> if (block.promptHint.blank()) "empty prompt hint" else null
        // A diagram that cannot be drawn is still shown as text (alt), so only a blank alt makes it unrenderable.
        is Diagram -> if (block.alt.blank()) "empty alt" else null
        is MissingComponent -> null
        is Unavailable -> null
    }

private fun choiceProblem(
    question: String,
    options: List<ChoiceOption>,
    correct: List<String>,
    single: Boolean,
): String? {
    val ids = options.map { it.id }
    return when {
        question.blank() -> "empty question"
        options.size < 2 -> "fewer than two options"
        options.any { it.text.blank() } -> "empty option"
        ids.hasDuplicates() -> "duplicate option ids"
        correct.isEmpty() || correct.any { it !in ids } -> "correct matches no option"
        !single && correct.hasDuplicates() -> "duplicate correct ids"
        !single && correct.size >= options.size -> "every option is correct"
        else -> null
    }
}

private fun fillBlankProblem(block: FillBlank): String? {
    val placeholders = placeholderIds(block.text)
    val blankIds = block.blanks.map { it.id }
    return when {
        block.text.blank() || block.blanks.isEmpty() -> "empty fill_blank"
        blankIds.hasDuplicates() -> "duplicate blank ids"
        placeholders.hasDuplicates() -> "repeated placeholder"
        placeholders.toSet() != blankIds.toSet() -> "placeholders and blanks do not match"
        block.blanks.any { b -> b.answers.none { it.isNotBlank() } } -> "blank without answers"
        else -> null
    }
}
