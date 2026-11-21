package com.teachme.feature.home

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.teachTween
import com.teachme.core.model.DueTopic
import com.teachme.core.model.REVIEW_SESSION_SIZE
import com.teachme.core.model.nextDuePhrase
import com.teachme.core.model.sessionMinutes
import com.teachme.core.ui.nextDueSentence
import java.time.Instant
import java.time.ZoneId

/**
 * "Ripasso di oggi" at the top of Home (home.md): a card when cards are due, one quiet line when nothing is due
 * but a card is coming, a retry line when the check failed, nothing otherwise. It slides in without moving the folders'
 * focus; nothing here is red, numbered as a badge or animated for attention.
 */
@Composable
internal fun ReviewEntry(
    review: HomeReview,
    selectedTopicId: String?,
    actions: HomeActions,
    modifier: Modifier = Modifier,
) {
    val spec = TeachTheme.motion.feedbackAppear
    val enter =
        if (TeachTheme.reducedMotion) {
            EnterTransition.None
        } else {
            fadeIn(teachTween(spec)) + expandVertically(teachTween(spec))
        }
    AnimatedVisibility(
        visible = review != HomeReview.Hidden,
        enter = enter,
        exit = ExitTransition.None,
        modifier = modifier,
    ) {
        when (review) {
            is HomeReview.Due -> ReviewDueCard(review, selectedTopicId, actions)
            is HomeReview.Upcoming -> ReviewQuietLine(review.nextDueAt)
            HomeReview.Failed -> ReviewFailedLine(actions.onRetryReview)
            HomeReview.Hidden -> Unit
        }
    }
}

@Composable
private fun ReviewDueCard(
    review: HomeReview.Due,
    selectedTopicId: String?,
    actions: HomeActions,
) {
    // The chips only exist with 2 topics or more; a stale selection counts as "Tutti".
    val showChips = review.topics.size >= 2
    val topic = if (showChips) review.topics.firstOrNull { it.topicId == selectedTopicId } else null
    // With a filter the numbers come from the topic, already in hand: no new request.
    val dueCount = topic?.dueCount ?: review.dueCount
    // A session is 20 cards: the card talks about the session, the second line about the rest (never "in ritardo").
    val session = dueCount.coerceAtMost(REVIEW_SESSION_SIZE)
    val minutes = sessionMinutes(dueCount)
    val cards = pluralStringResource(R.plurals.home_review_cards, session, session)
    val minutesText = pluralStringResource(R.plurals.home_review_minutes, minutes, minutes)
    val statusDescription = stringResource(R.string.home_review_status_a11y, cards, minutesText)
    TeachCard(Modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.lg)) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                TeachIcon(TeachIcons.Layers, contentDescription = null, tint = TeachTheme.colors.ink)
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(
                        stringResource(R.string.home_review_title),
                        modifier = Modifier.semantics { heading() },
                        style = TeachTheme.typography.ui.title3,
                    )
                    TeachText(
                        stringResource(R.string.home_review_status, cards, minutesText),
                        modifier =
                            Modifier.semantics {
                                liveRegion = LiveRegionMode.Polite
                                contentDescription = statusDescription
                            },
                        style = TeachTheme.typography.ui.body,
                        color = TeachTheme.colors.inkSoft,
                    )
                    if (dueCount > REVIEW_SESSION_SIZE) {
                        TeachText(
                            if (topic == null) {
                                stringResource(R.string.home_review_more, dueCount, REVIEW_SESSION_SIZE)
                            } else {
                                stringResource(
                                    R.string.home_review_more_topic,
                                    dueCount,
                                    topic.topicTitle,
                                    REVIEW_SESSION_SIZE,
                                )
                            },
                            style = TeachTheme.typography.ui.bodyS,
                            color = TeachTheme.colors.inkSoft,
                        )
                    }
                }
            }
            if (showChips) {
                ReviewTopicChips(review.topics, review.dueCount, topic?.topicId, actions.onSelectReviewTopic)
            }
            // The card itself is not tappable: only the button opens the review, so there is one target, not two.
            TeachButton(
                if (topic == null) {
                    stringResource(R.string.home_review_start)
                } else {
                    stringResource(R.string.home_review_start_topic, topic.topicTitle)
                },
                { actions.onOpenReview(topic?.topicId, topic?.topicTitle) },
                Modifier.fillMaxWidth(),
                contentDescription =
                    if (topic == null) {
                        stringResource(R.string.home_review_start_a11y, cards, minutesText)
                    } else {
                        stringResource(R.string.home_review_start_topic_a11y, topic.topicTitle, cards, minutesText)
                    },
            )
        }
    }
}

@Composable
private fun ReviewQuietLine(nextDueAt: Instant) {
    val resources = LocalResources.current
    val now = remember(nextDueAt) { Instant.now() }
    val sentence = resources.nextDueSentence(nextDuePhrase(nextDueAt, now, ZoneId.systemDefault()))
    Row(
        Modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.Top,
    ) {
        TeachIcon(
            TeachIcons.Layers,
            contentDescription = null,
            tint = TeachTheme.colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
        )
        TeachText(
            stringResource(R.string.home_review_none, sentence),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

@Composable
private fun ReviewFailedLine(onRetry: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(
            TeachIcons.CloudOff,
            contentDescription = null,
            tint = TeachTheme.colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
        )
        TeachText(
            stringResource(R.string.home_review_failed),
            modifier = Modifier.weight(1f),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
        TeachButton(stringResource(com.teachme.core.ui.R.string.ui_load_retry), onRetry, variant = ButtonVariant.Text)
    }
}

private val previewTopics = listOf(DueTopic("t1", "Rust", 8), DueTopic("t2", "SQL", 4))

@Composable
private fun Sample(review: HomeReview) =
    Column(Modifier.padding(TeachTheme.spacing.md)) { ReviewEntry(review, null, HomeActions()) }

@Preview(name = "Review due", showBackground = true)
@Composable
private fun DuePreview() = TeachTheme(darkTheme = false) { Sample(HomeReview.Due(12, previewTopics)) }

@Preview(name = "Review due dark", showBackground = true)
@Composable
private fun DueDarkPreview() = TeachTheme(darkTheme = true) { Sample(HomeReview.Due(34)) }

@Preview(name = "Review one card, font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun DueLargePreview() = TeachTheme(darkTheme = false) { Sample(HomeReview.Due(1)) }

@Preview(name = "Review quiet line", showBackground = true)
@Composable
private fun QuietPreview() =
    TeachTheme(darkTheme = false) {
        Sample(HomeReview.Upcoming(Instant.now().plusSeconds(172_800)))
    }

@Preview(name = "Review failed", showBackground = true)
@Composable
private fun FailedPreview() = TeachTheme(darkTheme = true) { Sample(HomeReview.Failed) }
