package com.teachme.sdui.logic

import com.teachme.sdui.inline.Inline
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CodeBlock
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.GlossaryTermBlock
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.model.PrimarySource
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

/**
 * What the context panel lists (docs/design/screens/lesson.md section 3.1): glossary terms and sources the
 * learner has already met. Only text that is visible before answering is scanned, so the panel can never
 * leak the answer of an exercise still to do: feedback, options and flashcard backs are excluded.
 */
object LessonIndex {
    private fun Block.visibleMarkdown(): List<String> =
        when (this) {
            is Paragraph -> listOf(text)
            is Callout -> listOf(text)
            is CodeBlock -> listOfNotNull(caption)
            is QuizSingle -> listOf(question)
            is QuizMulti -> listOf(question)
            is OrderSteps -> listOf(prompt)
            is OpenAnswer -> listOf(question)
            is PrimarySource -> listOf(why)
            is FillBlank -> emptyList()
            else -> emptyList()
        }

    /** Glossary keys met in [revealed] sections, in order of first mention, known keys only. */
    fun encounteredTerms(
        document: LessonDocument,
        revealed: List<Section>,
    ): List<String> {
        val keys = mutableListOf<String>()
        for (section in revealed) {
            for (block in section.items) {
                if (block is GlossaryTermBlock) keys += block.key
                keys +=
                    block
                        .visibleMarkdown()
                        .flatMap {
                            InlineMarkdown.parse(
                                it,
                            )
                        }.filterIsInstance<Inline.Term>()
                        .map { it.key }
            }
        }
        return keys.distinct().filter { document.glossaryEntry(it) != null }
    }

    /** Source keys cited or presented as primary in [revealed] sections, in order of first mention. */
    fun citedSources(
        document: LessonDocument,
        revealed: List<Section>,
    ): List<String> {
        val keys = mutableListOf<String>()
        for (section in revealed) {
            for (block in section.items) {
                if (block is PrimarySource) keys += block.source
                keys +=
                    block
                        .visibleMarkdown()
                        .flatMap {
                            InlineMarkdown.parse(
                                it,
                            )
                        }.filterIsInstance<Inline.Cite>()
                        .map { it.key }
            }
        }
        return keys.distinct().filter { document.source(it) != null }
    }
}
