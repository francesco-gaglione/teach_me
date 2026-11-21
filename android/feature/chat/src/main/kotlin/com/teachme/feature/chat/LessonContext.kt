package com.teachme.feature.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText

/** The lesson a chat was opened from (`ask_agent`, "Chiedi al Tutor"): the Tutor gets it as context for one message. */
data class ChatLessonContext(
    val lessonId: String,
    val number: Int,
    val title: String,
    val estimatedMinutes: Int = 0,
)

/**
 * The pill above the composer (ask_agent.md): "Sulla lezione 3: Lifetimes in pratica" with an `x` that removes it, so the
 * message goes without `lesson_id`. It says what the Tutor knows about this question and lets the learner take it away.
 */
@Composable
fun LessonContextChip(
    context: ChatLessonContext,
    onRemove: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val text = stringResource(R.string.chat_lesson_chip, context.number, context.title)
    val description = stringResource(R.string.chat_lesson_chip_a11y, context.number, context.title)
    Row(
        modifier
            .background(colors.surfaceSunken, TeachTheme.shapes.full)
            .padding(start = TeachTheme.spacing.sm)
            .semantics(mergeDescendants = false) { contentDescription = description },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachIcon(TeachIcons.BookOpen, null, tint = colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
        TeachText(text, Modifier.weight(1f, fill = false), style = TeachTheme.typography.ui.labelS, color = colors.ink)
        TeachIconButton(TeachIcons.X, stringResource(R.string.chat_lesson_chip_remove), onRemove, tint = colors.inkSoft)
    }
}

/** The block "Lezione" of the context pane, with the way back to the lesson (expanded windows only). */
@Composable
internal fun LessonPaneBlock(
    lesson: ChatLessonContext,
    onBackToLesson: () -> Unit,
) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(
            stringResource(R.string.chat_lesson_title),
            Modifier.semantics {
                heading()
            },
            style = TeachTheme.typography.ui.label,
        )
        TeachText(lesson.title, style = TeachTheme.typography.ui.title3)
        val meta =
            if (lesson.estimatedMinutes > 0) {
                stringResource(
                    R.string.chat_lesson_meta_minutes,
                    lesson.number,
                    pluralStringResource(
                        R.plurals.chat_lesson_minutes,
                        lesson.estimatedMinutes,
                        lesson.estimatedMinutes,
                    ),
                )
            } else {
                stringResource(R.string.chat_lesson_meta, lesson.number)
            }
        TeachText(meta, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
        TeachButton(stringResource(R.string.chat_lesson_back), onBackToLesson, variant = ButtonVariant.Secondary)
    }
}

@Composable
private fun ChipSample() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        LessonContextChip(ChatLessonContext("l", 3, "Lifetimes in pratica", 8), {})
        LessonPaneBlock(ChatLessonContext("l", 3, "Lifetimes in pratica", 8)) {}
    }
}

@Preview(name = "Lesson chip light", showBackground = true, widthDp = 360)
@Composable
private fun ChipLightPreview() = TeachTheme(darkTheme = false) { ChipSample() }

@Preview(name = "Lesson chip dark", showBackground = true, widthDp = 360)
@Composable
private fun ChipDarkPreview() = TeachTheme(darkTheme = true) { ChipSample() }

@Preview(name = "Lesson chip font 2.0", showBackground = true, widthDp = 360, heightDp = 400, fontScale = 2f)
@Composable
private fun ChipLargePreview() = TeachTheme(darkTheme = false) { ChipSample() }
