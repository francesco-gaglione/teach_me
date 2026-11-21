package com.teachme.feature.lesson

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.window.Popup
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText
import kotlinx.coroutines.delay

/** The indicator only shows when the queue has been non-empty this long, so a good network never flickers it. */
internal const val QUEUE_INDICATOR_DELAY_MS = 5_000L

/** How long the spoken confirmation stays in the tree before it is removed, so the same text can be said again. */
internal const val ANNOUNCEMENT_MS = 3_000L

/** One thing to say; a class without `equals` so that two with the same text are two announcements. */
private class Announcement(
    @StringRes val text: Int,
)

/**
 * `cloud-off` in the top bar plus its popover (lesson.md section 8.1): "Alcune risposte non sono ancora inviate" and
 * "Riprova ora". One indicator for everything that is queued: answers (after [QUEUE_INDICATOR_DELAY_MS]) and, when
 * [highlightsFailing], the highlights too (highlight.md section 11: the 30 s are already counted), each with its own
 * line; the handwritten notes have theirs too ([notesFailing]: not sent for more than 30 s; [notesTooLarge]: the backend
 * refused them for their size, shown at once because waiting does not help; handwriting.md section 9). Not drawn at all
 * while there is nothing to say. Says "Risposte inviate." / "Evidenziazioni salvate." / "Note a mano salvate." (polite)
 * when what it was about is sent.
 */
@Composable
fun QueueIndicator(
    queued: Int,
    onRetryNow: () -> Unit,
    highlightsFailing: Boolean = false,
    notesFailing: Boolean = false,
    notesTooLarge: Boolean = false,
) {
    var answersShown by remember { mutableStateOf(false) }
    var open by remember { mutableStateOf(false) }
    // What was just sent, to be said once (polite); the last one wins.
    var announcement by remember { mutableStateOf<Announcement?>(null) }
    val nonEmpty = queued > 0
    LaunchedEffect(nonEmpty) {
        if (nonEmpty) {
            delay(QUEUE_INDICATOR_DELAY_MS)
            answersShown = true
        } else {
            if (answersShown) announcement = Announcement(R.string.lesson_queue_sent)
            answersShown = false
        }
    }
    var highlightsWereFailing by remember { mutableStateOf(false) }
    LaunchedEffect(highlightsFailing) {
        if (!highlightsFailing && highlightsWereFailing) announcement = Announcement(R.string.lesson_highlight_saved)
        highlightsWereFailing = highlightsFailing
    }
    val notesShown = notesFailing || notesTooLarge
    var notesWereShown by remember { mutableStateOf(false) }
    LaunchedEffect(notesShown) {
        if (!notesShown && notesWereShown) announcement = Announcement(R.string.lesson_ink_saved)
        notesWereShown = notesShown
    }
    val shown = answersShown || highlightsFailing || notesShown
    LaunchedEffect(shown) { if (!shown) open = false }
    // The live region is announced when it appears or its text changes: the node goes away after a while and each
    // announcement is its own instance, so a second success with the same text is spoken again.
    LaunchedEffect(announcement) {
        if (announcement != null) {
            delay(ANNOUNCEMENT_MS)
            announcement = null
        }
    }
    announcement?.let { current ->
        val spoken = stringResource(current.text)
        key(current) {
            Box(
                Modifier.semantics {
                    liveRegion = LiveRegionMode.Polite
                    contentDescription = spoken
                },
            )
        }
    }
    if (!shown) return
    val currentRetry by rememberUpdatedState(onRetryNow)
    Box {
        TeachIconButton(
            TeachIcons.CloudOff,
            stringResource(
                when {
                    answersShown -> R.string.lesson_queue_description
                    highlightsFailing -> R.string.lesson_highlight_queue_description
                    else -> R.string.lesson_ink_queue_description
                },
            ),
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
                    verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    if (answersShown) {
                        TeachText(stringResource(R.string.lesson_queue_body), style = TeachTheme.typography.ui.body)
                    }
                    if (highlightsFailing) {
                        TeachText(
                            stringResource(R.string.lesson_highlight_queue_body),
                            style = TeachTheme.typography.ui.body,
                        )
                    }
                    if (notesFailing) {
                        TeachText(
                            stringResource(R.string.lesson_ink_queue_body),
                            style = TeachTheme.typography.ui.body,
                        )
                    }
                    if (notesTooLarge) {
                        TeachText(
                            stringResource(R.string.lesson_ink_queue_too_large),
                            style = TeachTheme.typography.ui.body,
                        )
                    }
                    TeachButton(
                        stringResource(R.string.lesson_queue_retry),
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

/**
 * Says something once, politely, for a screen reader (the undo and redo of the notes): a node with a live region that
 * is there for [ANNOUNCEMENT_MS] and then goes away, so the same words can be said again.
 */
@androidx.compose.runtime.Stable
internal class PoliteAnnouncer {
    private class Spoken(
        val text: String,
    )

    private var current by mutableStateOf<Spoken?>(null)

    fun say(text: String) {
        current = Spoken(text)
    }

    @Composable
    fun Run() {
        val spoken = current ?: return
        LaunchedEffect(spoken) {
            delay(ANNOUNCEMENT_MS)
            if (current === spoken) current = null
        }
        key(spoken) {
            Box(
                Modifier.semantics {
                    liveRegion = LiveRegionMode.Polite
                    contentDescription = spoken.text
                },
            )
        }
    }
}
