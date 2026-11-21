package com.teachme.feature.review

import android.content.res.Resources
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachErrorState
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.LoadError
import com.teachme.core.model.NextDuePhrase
import com.teachme.core.model.dayMonthFormatter
import com.teachme.core.model.nextDuePhrase
import com.teachme.core.ui.intervalText
import com.teachme.core.ui.nextDueSentence
import com.teachme.sdui.inline.InlineText
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale
import com.teachme.core.ui.R as UiR

private const val SKELETON_LINES = 3
private val SKELETON_CARD_HEIGHT = 160.dp
private val SKELETON_BUTTON_HEIGHT = 56.dp
private val SKELETON_LINE_HEIGHT = 14.dp

/** Skeleton while the session loads (review.md section 11): one accessibility element, "Caricamento delle schede". */
@Composable
internal fun ReviewLoading(
    slow: Boolean,
    modifier: Modifier = Modifier,
) {
    val description = stringResource(R.string.review_loading)
    val state = stringResource(UiR.string.ui_loading_state)
    Column(
        modifier.fillMaxWidth().padding(vertical = TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        Column(
            Modifier.clearAndSetSemantics {
                contentDescription = description
                stateDescription = state
            },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            SkeletonBar(0.2f, SKELETON_LINE_HEIGHT)
            Column(
                Modifier
                    .fillMaxWidth()
                    .height(SKELETON_CARD_HEIGHT)
                    .background(TeachTheme.colors.surface, TeachTheme.shapes.md)
                    .padding(TeachTheme.spacing.md),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            ) {
                repeat(SKELETON_LINES) { SkeletonBar(if (it == SKELETON_LINES - 1) 0.5f else 1f, SKELETON_LINE_HEIGHT) }
            }
            SkeletonBar(1f, SKELETON_BUTTON_HEIGHT)
        }
        if (slow) {
            TeachText(
                stringResource(UiR.string.ui_still_loading),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

@Composable
private fun SkeletonBar(
    fraction: Float,
    height: androidx.compose.ui.unit.Dp,
) {
    Box(
        Modifier
            .fillMaxWidth(fraction)
            .height(height)
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.sm),
    )
}

/** Error of the first load (review.md section 11): "Riprova" is the only primary, "Torna alla Home" a text button. */
@Composable
internal fun ReviewLoadError(
    error: LoadError,
    onRetry: () -> Unit,
    onHome: () -> Unit,
) {
    val title: String
    val body: String
    when (error) {
        // A 401 never gets its own message (login.md): the sign-in opens over this screen and the retry reloads it.
        LoadError.Network, LoadError.Unauthorized -> {
            title = stringResource(UiR.string.ui_load_network_title)
            body = stringResource(UiR.string.ui_load_network_body)
        }
        LoadError.Server -> {
            title = stringResource(UiR.string.ui_load_server_title)
            body = stringResource(UiR.string.ui_load_server_body)
        }
    }
    TeachErrorState(
        title = title,
        body = body,
        primaryLabel = stringResource(UiR.string.ui_load_retry),
        onPrimary = onRetry,
        secondaryLabel = stringResource(R.string.review_back_home),
        onSecondary = onHome,
    )
}

/** Nothing is due (review.md section 11): a statement, never praise. */
@Composable
internal fun ReviewEmpty(
    nextDueAt: Instant?,
    now: Instant,
    onHome: () -> Unit,
    modifier: Modifier = Modifier,
    topicTitle: String? = null,
) {
    val resources = LocalResources.current
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Column(
        modifier.fillMaxSize().padding(vertical = TeachTheme.spacing.xl),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TeachText(
            if (topicTitle != null) {
                stringResource(R.string.review_topic_empty_title, topicTitle)
            } else {
                stringResource(if (nextDueAt != null) R.string.review_empty_title else R.string.review_none_title)
            },
            modifier = Modifier.focusRequester(focus).focusable().semantics { heading() },
            style = TeachTheme.typography.ui.title1,
            textAlign = TextAlign.Center,
        )
        TeachText(
            if (nextDueAt != null) {
                topicNextDueSentence(nextDueAt, now, topicTitle)
            } else if (topicTitle != null) {
                stringResource(R.string.review_topic_no_cards)
            } else {
                stringResource(R.string.review_none_body)
            },
            style = TeachTheme.typography.ui.bodyL,
            color = TeachTheme.colors.inkSoft,
            textAlign = TextAlign.Center,
        )
        TeachButton(
            stringResource(R.string.review_back_home),
            onHome,
            size = ButtonSize.Large,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/**
 * End of the session (review.md section 7): what was done, not how well. No percentages, no praise, no streak. The
 * grades "Non la ricordavo" are listed because they come back soon: information, not blame.
 */
@Composable
internal fun FinishContent(
    summary: FinishSummary,
    now: Instant,
    onHome: () -> Unit,
    onMore: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val resources = LocalResources.current
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Column(
        modifier.fillMaxWidth().padding(vertical = TeachTheme.spacing.lg),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachText(
            stringResource(R.string.review_finish_title),
            modifier = Modifier.focusRequester(focus).focusable().semantics { heading() },
            style = TeachTheme.typography.ui.title1,
        )
        if (summary.reviewed > 0) {
            TeachText(
                if (summary.topicTitle != null) {
                    pluralStringResource(
                        R.plurals.review_topic_finish_count,
                        summary.reviewed,
                        summary.reviewed,
                        summary.topicTitle,
                    )
                } else {
                    pluralStringResource(R.plurals.review_finish_count, summary.reviewed, summary.reviewed)
                },
                style = TeachTheme.typography.ui.bodyL,
            )
        }
        if (summary.conflicts > 0) {
            TeachText(
                pluralStringResource(R.plurals.review_finish_conflicts, summary.conflicts, summary.conflicts),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
        if (summary.again.isNotEmpty()) AgainList(summary)
        if (summary.hasBacklog) {
            val residual = summary.residual ?: 0
            TeachText(
                if (summary.topicTitle != null) {
                    pluralStringResource(R.plurals.review_topic_finish_backlog, residual, residual, summary.topicTitle)
                } else {
                    pluralStringResource(R.plurals.review_finish_backlog, residual, residual)
                },
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(
                if (summary.nextSession == 1) {
                    stringResource(R.string.review_finish_last)
                } else {
                    stringResource(R.string.review_finish_more, summary.nextSession)
                },
                onMore,
                modifier = Modifier.fillMaxWidth(),
                variant = ButtonVariant.Secondary,
            )
        } else if (summary.nextDueAt != null) {
            TeachText(
                topicNextDueSentence(summary.nextDueAt, now, summary.topicTitle),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
            )
        }
        TeachButton(
            stringResource(R.string.review_back_home),
            onHome,
            modifier = Modifier.fillMaxWidth(),
            size = ButtonSize.Large,
        )
    }
}

/** "La prossima scheda arriva domani." or, in a filtered session, "La prossima scheda di Rust arriva domani." */
@Composable
private fun topicNextDueSentence(
    nextDueAt: Instant,
    now: Instant,
    topicTitle: String?,
): String {
    val resources = LocalResources.current
    if (topicTitle == null) return resources.nextDueSentence(nextDuePhrase(nextDueAt, now, ZoneId.systemDefault()))
    return when (val phrase = nextDuePhrase(nextDueAt, now, ZoneId.systemDefault())) {
        NextDuePhrase.Tomorrow -> resources.getString(R.string.review_topic_next_due_tomorrow, topicTitle)
        is NextDuePhrase.TodayAt ->
            resources.getString(
                R.string.review_topic_next_due_today,
                topicTitle,
                phrase.time.format(DateTimeFormatter.ofPattern("HH:mm")),
            )
        else -> resources.getString(R.string.review_topic_next_due_on, topicTitle, resources.dueWhen(phrase))
    }
}

/** The "when" of "arriva …" for the phrases that read as a complement: "tra 10 minuti", "lunedì", "il 24 novembre". */
private fun Resources.dueWhen(phrase: NextDuePhrase): String {
    val locale = configuration.locales.get(0) ?: Locale.getDefault()
    return when (phrase) {
        is NextDuePhrase.In -> intervalText(phrase.interval)
        is NextDuePhrase.OnWeekday -> phrase.day.getDisplayName(TextStyle.FULL, locale)
        is NextDuePhrase.OnDate ->
            getString(UiR.string.ui_next_due_date, phrase.date.format(dayMonthFormatter(locale)))
        else -> ""
    }
}

/** The topic is gone (review.md section 7.3): information, not a network error. */
@Composable
internal fun ReviewTopicGone(
    onAll: () -> Unit,
    onHome: () -> Unit,
) {
    Column(
        Modifier.fillMaxSize().padding(TeachTheme.layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        TeachStatusPanel(
            StatusVariant.Info,
            stringResource(R.string.review_topic_gone_title),
            body = stringResource(R.string.review_topic_gone_body),
            modifier = Modifier.widthIn(max = TeachTheme.layout.readingMaxWidth),
        )
        TeachButton(stringResource(R.string.review_topic_gone_all), onAll, size = ButtonSize.Large)
        TeachButton(stringResource(R.string.review_back_home), onHome, variant = ButtonVariant.Text)
    }
}

@Composable
private fun AgainList(summary: FinishSummary) {
    val resources = LocalResources.current
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(stringResource(R.string.review_finish_again_title), style = TeachTheme.typography.ui.label)
        summary.again.forEach { vote ->
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                TeachText("·", style = TeachTheme.typography.ui.body)
                InlineText(
                    vote.card.front,
                    style = TeachTheme.typography.ui.body,
                    interactive = false,
                    maxLines = 2,
                    modifier = Modifier.weight(1f),
                )
                resources.intervalText(vote.intervalSeconds)?.let {
                    TeachText(it, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
                }
            }
        }
        if (summary.againMore > 0) {
            TeachText(
                pluralStringResource(R.plurals.review_finish_again_more, summary.againMore, summary.againMore),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    }
}

/** The reading column of every state of the screen: 600 dp at most, centred, the margins of the width class. */
@Composable
internal fun ReviewColumn(
    modifier: Modifier = Modifier,
    scroll: Boolean = true,
    content: @Composable () -> Unit,
) {
    Box(modifier.fillMaxSize().padding(horizontal = screenMargin()), contentAlignment = Alignment.TopCenter) {
        Box(
            Modifier
                .widthIn(max = TeachTheme.layout.readingMaxWidth)
                .fillMaxSize()
                .then(if (scroll) Modifier.verticalScroll(rememberScrollState()) else Modifier),
        ) { content() }
    }
}
