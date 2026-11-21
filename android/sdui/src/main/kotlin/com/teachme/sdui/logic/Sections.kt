package com.teachme.sdui.logic

import com.teachme.sdui.model.Block
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.model.MissingComponent
import com.teachme.sdui.model.Unavailable

/**
 * A reveal unit of the lesson (docs/design/screens/lesson.md section 2). [number] is the 1-based position
 * among the level 2 sections ("Sezione k di N"), null for the introduction (the blocks before the first
 * level 2 heading).
 */
data class Section(
    val index: Int,
    val number: Int?,
    val title: String?,
    /** What is drawn, in order: `missing_component` already replaced by its fallback, neutral rows merged. */
    val items: List<Block>,
) {
    /** Every exercise that is drawn in this section (fallback ones included). */
    val exercises: List<Exercise> get() = items.filterIsInstance<Exercise>()

    val requiredExercises: List<Exercise> get() = exercises.filter { it.required }

    val isIntro: Boolean get() = number == null
}

data class LessonSections(
    val sections: List<Section>,
) {
    /** N: how many level 2 sections the lesson has. 0 means a single section and no progress. */
    val numberedCount: Int get() = sections.count { it.number != null }

    val showsProgress: Boolean get() = numberedCount > 0
}

/** Splits a document into sections. Pure: the same document always gives the same sections. */
object SectionBuilder {
    fun build(document: LessonDocument): LessonSections {
        val groups = mutableListOf<MutableList<Block>>()
        val headings = mutableListOf<Heading?>()
        groups += mutableListOf<Block>()
        headings += null
        for (block in document.blocks) {
            if (block is Heading && block.level == 2) {
                groups += mutableListOf<Block>(block)
                headings += block
            } else {
                groups.last() += block
            }
        }
        val sections = mutableListOf<Section>()
        var number = 0
        for ((i, group) in groups.withIndex()) {
            val heading = headings[i]
            val items = display(group)
            // An empty introduction (the lesson starts with a level 2 heading) is not a section.
            if (heading == null && items.isEmpty()) continue
            sections +=
                Section(
                    index = sections.size,
                    number = if (heading != null) ++number else null,
                    title = heading?.text,
                    items = items,
                )
        }
        return LessonSections(sections)
    }

    /** Replaces each `missing_component` by its drawable fallback and merges consecutive neutral rows. */
    fun display(blocks: List<Block>): List<Block> {
        val flat =
            blocks.flatMap { block ->
                if (block is MissingComponent) {
                    val drawable = block.fallback.filter { it !is Unavailable }
                    drawable.ifEmpty {
                        listOf(
                            Unavailable(block.id, block.type, com.teachme.sdui.model.UnavailableReason.NoFallback),
                        )
                    }
                } else {
                    listOf(block)
                }
            }
        val out = mutableListOf<Block>()
        for (block in flat) {
            if (block is Unavailable && out.lastOrNull() is Unavailable) continue
            out += block
        }
        return out
    }
}
