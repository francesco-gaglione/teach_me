package com.teachme.sdui.highlight

import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.LessonDocument
import com.teachme.sdui.render.ExerciseStates
import com.teachme.sdui.render.LessonLinkTarget
import com.teachme.sdui.render.SduiEnvironment
import com.teachme.sdui.render.SduiHost

/** A highlighter that only holds a list: enough to draw highlights on a real device. */
class StaticHighlights(
    override val highlights: List<Highlight> = emptyList(),
) : HighlightHost {
    override val color = HighlightColor.YELLOW

    override fun add(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun erase(
        blockId: String,
        start: Int,
        end: Int,
    ) = Unit

    override fun remove(
        blockId: String,
        position: Int,
    ) = Unit
}

fun deviceEnvironment(blocks: List<Block>): SduiEnvironment {
    val states =
        object : ExerciseStates {
            override fun get(blockId: String) = ExerciseState()

            override fun update(
                blockId: String,
                transform: (ExerciseState) -> ExerciseState,
            ) = Unit

            override fun isLocked(blockId: String) = false
        }
    val host =
        object : SduiHost {
            override fun openTerm(key: String) = Unit

            override fun openSource(key: String) = Unit

            override fun openUrl(url: String) = Unit

            override fun askAgent(promptHint: String) = Unit

            override fun openLesson(lessonId: String) = Unit

            override fun lessonLinkTarget(lessonId: String): LessonLinkTarget? = null

            override fun copyText(text: String) = Unit
        }
    return SduiEnvironment(LessonDocument(1, "T", "O", 5, emptyList(), emptyList(), blocks), host, states)
}
