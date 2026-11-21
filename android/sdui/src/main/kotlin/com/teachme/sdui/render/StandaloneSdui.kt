package com.teachme.sdui.render

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.model.LessonDocument

private val EMPTY_DOCUMENT =
    LessonDocument(
        catalogVersion = 1,
        title = "",
        objective = "",
        estimatedMinutes = 0,
        glossary = emptyList(),
        sources = emptyList(),
        blocks = emptyList(),
    )

private object InertHost : SduiHost {
    override fun openTerm(key: String) = Unit

    override fun openSource(key: String) = Unit

    override fun openUrl(url: String) = Unit

    override fun askAgent(promptHint: String) = Unit

    override fun openLesson(lessonId: String) = Unit

    override fun lessonLinkTarget(lessonId: String): LessonLinkTarget? = null

    override fun copyText(text: String) = Unit
}

private object NoStates : ExerciseStates {
    override fun get(blockId: String) = ExerciseState()

    override fun update(
        blockId: String,
        transform: (ExerciseState) -> ExerciseState,
    ) = Unit

    override fun isLocked(blockId: String) = true
}

/**
 * Lets [content] draw inline markdown (`InlineText`) outside a lesson, for example the text of a learning record or a
 * glossary term: no terms or citations to open, nothing tappable. The environment is empty and inert.
 */
@Composable
fun ProvideStandaloneSdui(content: @Composable () -> Unit) {
    val environment = remember { SduiEnvironment(EMPTY_DOCUMENT, InertHost, NoStates) }
    CompositionLocalProvider(LocalSdui provides environment, content = content)
}
