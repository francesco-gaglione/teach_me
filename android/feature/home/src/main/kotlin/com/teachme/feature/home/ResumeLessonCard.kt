package com.teachme.feature.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonInProgress
import com.teachme.core.model.LessonStatus

/**
 * "Riprendi la lezione" under the review card (home.md, Fase 8b). Absent when there is nothing to resume or the
 * request failed: never a placeholder, never a banner. Only the button is tappable.
 */
@Composable
internal fun ResumeLessonCard(
    resume: LessonInProgress?,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (resume == null) return
    val lesson = resume.lesson
    val minutes = pluralStringResource(R.plurals.home_review_minutes, lesson.estimatedMinutes, lesson.estimatedMinutes)
    TeachCard(modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.lg)) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                TeachIcon(TeachIcons.BookOpen, contentDescription = null, tint = TeachTheme.colors.ink)
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(
                        lesson.title,
                        modifier = Modifier.semantics { heading() },
                        style = TeachTheme.typography.ui.title3,
                    )
                    TeachText(
                        resume.topicTitle,
                        style = TeachTheme.typography.ui.body,
                        color = TeachTheme.colors.inkSoft,
                    )
                    TeachText(
                        stringResource(R.string.home_resume_status, lesson.number, minutes),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
            }
            TeachButton(
                stringResource(R.string.home_resume_title_button),
                { onOpen(lesson.id) },
                Modifier.fillMaxWidth(),
                variant = ButtonVariant.Secondary,
                contentDescription =
                    stringResource(
                        R.string.home_resume_a11y,
                        lesson.title,
                        resume.topicTitle,
                        lesson.number,
                        minutes,
                    ),
            )
        }
    }
}

private val sample =
    LessonInProgress(
        Lesson("l1", "t1", 3, "Ownership e borrowing", "", 8, LessonStatus.InProgress, 1, 1, null, null),
        "Rust in pratica",
        null,
    )

@Preview(name = "Resume", showBackground = true)
@Composable
private fun ResumePreview() =
    TeachTheme(darkTheme = false) { Column(Modifier.padding(TeachTheme.spacing.md)) { ResumeLessonCard(sample, {}) } }

@Preview(name = "Resume dark font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun ResumeDarkPreview() =
    TeachTheme(darkTheme = true) { Column(Modifier.padding(TeachTheme.spacing.md)) { ResumeLessonCard(sample, {}) } }
