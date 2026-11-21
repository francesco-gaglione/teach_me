package com.teachme.feature.lesson

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText

private const val LARGE_FONT_TITLE_LINES = 2
private const val LARGE_FONT_SCALE = 1.5f

/** The "Ascolta la lezione" action (lesson-audio.md section 3): [enabled] false while no block has its audio yet. */
class LessonListenAction(
    val enabled: Boolean,
    val onClick: () -> Unit,
)

/**
 * Lesson top bar (docs/design/screens/lesson.md section 3): close on the left, title and the "Sezione k di N"
 * subtitle, optional "Indice" button when the context panel is not on screen, and a 4 dp progress bar on
 * its bottom edge. The bar always comes with words (the subtitle).
 */
@Composable
fun LessonTopBar(
    title: String,
    subtitle: String?,
    progress: Float,
    showIndexButton: Boolean,
    onClose: () -> Unit,
    onIndex: () -> Unit,
    modifier: Modifier = Modifier,
    /** The "risposte da inviare" indicator (lesson.md section 8.1); null when the queue is empty. */
    queueIndicator: (@Composable () -> Unit)? = null,
    /** "Ascolta la lezione", left of the index; null when the feature is off or the audio strip is on screen. */
    listen: LessonListenAction? = null,
) {
    val colors = TeachTheme.colors
    val largeFont = LocalDensity.current.fontScale >= LARGE_FONT_SCALE
    val motion = TeachTheme.motion.progress
    val animated by animateFloatAsState(
        targetValue = progress,
        animationSpec = tween(if (TeachTheme.reducedMotion) 0 else motion.durationMs, easing = motion.easing),
        label = "lesson-progress",
    )
    Column(modifier.fillMaxWidth().background(colors.bg)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = TeachTheme.layout.topBarHeight)
                .padding(horizontal = TeachTheme.spacing.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeachIconButton(TeachIcons.X, stringResource(R.string.lesson_close), onClose, tint = colors.ink)
            Column(Modifier.weight(1f).padding(horizontal = TeachTheme.spacing.xs)) {
                TeachText(
                    title,
                    Modifier.semantics { heading() },
                    style = TeachTheme.typography.ui.title3,
                    maxLines = if (largeFont) LARGE_FONT_TITLE_LINES else 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (subtitle != null) {
                    TeachText(subtitle, style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                }
            }
            queueIndicator?.invoke()
            if (listen != null) {
                TeachIconButton(
                    TeachIcons.Volume2,
                    stringResource(
                        if (listen.enabled) {
                            R.string.lesson_audio_listen_lesson
                        } else {
                            R.string.lesson_audio_listen_lesson_preparing
                        },
                    ),
                    listen.onClick,
                    enabled = listen.enabled,
                    tint = if (listen.enabled) colors.ink else colors.inkSoft,
                )
            }
            if (showIndexButton) {
                TeachIconButton(
                    TeachIcons.List,
                    stringResource(R.string.lesson_index_button),
                    onIndex,
                    tint = colors.ink,
                )
            }
        }
        val description = stringResource(R.string.lesson_progress_a11y)
        Box(
            Modifier
                .fillMaxWidth()
                .height(PROGRESS_HEIGHT)
                .background(colors.line)
                .semantics {
                    contentDescription = description
                    progressBarRangeInfo = ProgressBarRangeInfo(progress.coerceIn(0f, 1f), 0f..1f)
                    if (subtitle != null) stateDescription = subtitle
                }.drawBehind {
                    drawRect(
                        colors.accent,
                        size = Size(size.width * animated.coerceIn(0f, 1f), size.height),
                    )
                },
        )
    }
}

private val PROGRESS_HEIGHT = 4.dp

@Composable
private fun TopBarSample() {
    Column(Modifier.background(TeachTheme.colors.bg)) {
        LessonTopBar("Ownership e move", "Sezione 2 di 4", 0.4f, showIndexButton = true, onClose = {
        }, onIndex = {}, listen = LessonListenAction(true) {})
        LessonTopBar(
            "Una lezione senza sezioni",
            null,
            0f,
            showIndexButton = false,
            onClose = {},
            onIndex = {},
            listen = LessonListenAction(enabled = false) {},
        )
    }
}

@Preview(name = "Lesson top bar light", showBackground = true)
@Composable
private fun TopBarLightPreview() = TeachTheme(darkTheme = false) { TopBarSample() }

@Preview(name = "Lesson top bar dark", showBackground = true)
@Composable
private fun TopBarDarkPreview() = TeachTheme(darkTheme = true) { TopBarSample() }

@Preview(name = "Lesson top bar font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun TopBarLargeFontPreview() = TeachTheme(darkTheme = false) { TopBarSample() }
