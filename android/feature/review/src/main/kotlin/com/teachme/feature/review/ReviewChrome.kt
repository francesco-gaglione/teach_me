package com.teachme.feature.review

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.teachTween
import com.teachme.core.ui.intervalTextOrNull
import kotlinx.coroutines.delay

private val UNDO_ROW_HEIGHT = 56.dp
private val PROGRESS_BAR_HEIGHT = 4.dp

/** The indicator only shows once the queue has been non-empty this long, so a good network never flickers it. */
internal const val QUEUE_INDICATOR_DELAY_MS = 5_000L

/** "3 di 12" and a 4 dp bar: progress of the session, not a goal (it never changes colour or celebrates). */
@Composable
internal fun ProgressRow(
    position: Int,
    total: Int,
    modifier: Modifier = Modifier,
) {
    val fraction by animateFloatAsState(
        targetValue = if (total == 0) 0f else position.toFloat() / total,
        animationSpec = if (TeachTheme.reducedMotion) snap() else teachTween(TeachTheme.motion.progress),
        label = "review-progress",
    )
    val text = stringResource(R.string.review_progress, position, total)
    Column(
        modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) {
                contentDescription = text
                progressBarRangeInfo = ProgressBarRangeInfo(position.toFloat(), 0f..total.toFloat())
            },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        Box(
            Modifier
                .fillMaxWidth()
                .height(
                    PROGRESS_BAR_HEIGHT,
                ).background(TeachTheme.colors.line, TeachTheme.shapes.full),
        ) {
            Box(
                Modifier
                    .fillMaxWidth(fraction)
                    .height(PROGRESS_BAR_HEIGHT)
                    .background(TeachTheme.colors.accent, TeachTheme.shapes.full),
            )
        }
    }
}

/**
 * The row under the progress (review.md section 5): 56 dp always reserved, so it appearing never moves the card. Plain
 * text on the background, no icon and no colour of outcome; a polite live region announces it.
 */
@Composable
internal fun UndoRow(
    notice: ReviewNotice?,
    onUndo: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxWidth().heightIn(min = UNDO_ROW_HEIGHT), contentAlignment = Alignment.CenterStart) {
        // Keeps the last text while it fades out.
        var shown by remember { mutableStateOf(notice) }
        if (notice != null) shown = notice
        val fade = if (TeachTheme.reducedMotion) 0 else TeachTheme.motion.cardReveal.durationMs
        AnimatedVisibility(
            visible = notice != null,
            enter = fadeIn(tween(fade)),
            exit = fadeOut(tween(fade)),
        ) {
            val current = shown ?: return@AnimatedVisibility
            val text = noticeText(current)
            Row(
                Modifier
                    .fillMaxWidth()
                    .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachText(
                    text,
                    modifier = Modifier.weight(1f),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                    maxLines = 2,
                )
                if (current is ReviewNotice.Voted) {
                    TeachButton(stringResource(R.string.review_undo_action), onUndo, variant = ButtonVariant.Text)
                }
            }
        }
    }
}

@Composable
internal fun noticeText(notice: ReviewNotice): String =
    when (notice) {
        is ReviewNotice.Voted -> {
            val interval = intervalTextOrNull(notice.intervalSeconds)
            if (interval != null) {
                stringResource(R.string.review_undo_voted, ratingText(notice.rating), interval)
            } else {
                stringResource(R.string.review_undo_voted_plain, ratingText(notice.rating))
            }
        }
        ReviewNotice.AlreadyChanged -> stringResource(R.string.review_undo_changed)
        ReviewNotice.UndoGone -> stringResource(R.string.review_undo_gone)
        ReviewNotice.UndoOffline -> stringResource(R.string.review_undo_offline)
        ReviewNotice.Suspended -> stringResource(R.string.review_undo_suspended)
    }

/**
 * `cloud-off` in the top bar plus its popover (review.md section 8): shown only when grades have been waiting for more
 * than 5 s. The number is only in the description. Says "Voti inviati." (polite) when the queue empties.
 */
@Composable
internal fun ReviewQueueIndicator(
    queued: Int,
    onRetryNow: () -> Unit,
) {
    var visible by remember { mutableStateOf(false) }
    var open by remember { mutableStateOf(false) }
    val nonEmpty = queued > 0
    LaunchedEffect(nonEmpty) {
        if (nonEmpty) {
            delay(QUEUE_INDICATOR_DELAY_MS)
            visible = true
        } else {
            visible = false
            open = false
        }
    }
    if (!visible) return
    val currentRetry by rememberUpdatedState(onRetryNow)
    Box {
        TeachIconButton(
            TeachIcons.CloudOff,
            stringResource(R.string.review_queue_description, queued),
            { open = !open },
            tint = TeachTheme.colors.inkSoft,
        )
        if (open) {
            Popup(alignment = Alignment.TopEnd, onDismissRequest = { open = false }) {
                Column(
                    Modifier
                        .padding(top = TeachTheme.layout.minTouchTarget)
                        .widthIn(max = TeachTheme.layout.contextPaneWidth)
                        .border(TeachTheme.stroke.hairline, TeachTheme.colors.line, TeachTheme.shapes.md)
                        .background(TeachTheme.colors.surfaceRaised, TeachTheme.shapes.md)
                        .padding(TeachTheme.spacing.md),
                ) {
                    TeachText(stringResource(R.string.review_queue_body), style = TeachTheme.typography.ui.body)
                    TeachButton(
                        stringResource(R.string.review_queue_retry),
                        {
                            currentRetry()
                            open = false
                        },
                        variant = ButtonVariant.Text,
                    )
                }
            }
        }
    }
}

/** Invisible polite live region that says "Voti inviati." while [visible]. */
@Composable
internal fun SentAnnouncement(visible: Boolean) {
    if (!visible) return
    val text = stringResource(R.string.review_queue_sent)
    Box(
        Modifier
            .height(1.dp)
            .graphicsLayer { alpha = 0f }
            .semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
    )
}
