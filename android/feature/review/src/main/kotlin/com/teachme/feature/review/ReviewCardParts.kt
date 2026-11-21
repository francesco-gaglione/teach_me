package com.teachme.feature.review

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.snap
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.designsystem.components.teachTween
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewRating
import com.teachme.core.ui.intervalTextOrNull
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.render.KeyHint

private val CARD_MIN_HEIGHT_WIDE = 200.dp
private val CARD_MIN_HEIGHT_COMPACT = 160.dp
private val RATING_MIN_HEIGHT = 64.dp
private val OPEN_LESSON_MIN_HEIGHT = 48.dp
private val OPEN_LESSON_ICON = 20.dp
private const val FRONT_PREVIEW_LINES = 3
private const val CAPTION_MAX_LINES = 2

/** Plain text of an inline-markdown field, for screen readers. */
internal fun plainText(markdown: String): String = InlineMarkdown.plainText(markdown) { null }

private fun cardPadding(width: WindowWidthClass): Dp =
    when (width) {
        WindowWidthClass.Expanded -> 24.dp
        WindowWidthClass.Medium -> 20.dp
        WindowWidthClass.Compact -> 16.dp
    }

@Composable
private fun reviewFade(): FiniteAnimationSpec<Float> =
    if (TeachTheme.reducedMotion) snap() else teachTween(TeachTheme.motion.cardReveal)

/** The front or the back of a card (review.md section 4). The change is a 150 ms cross-fade, never a flip. */
@Composable
internal fun ReviewCardView(
    card: ReviewCard,
    position: Int,
    total: Int,
    revealed: Boolean,
    showHint: Boolean,
    focus: FocusRequester,
    modifier: Modifier = Modifier,
    firstOfSession: Boolean = false,
    hardwareKeyboard: Boolean = false,
    onOpenLesson: (String) -> Unit = {},
) {
    val width = currentWindowWidthClass()
    val minHeight = if (width == WindowWidthClass.Compact) CARD_MIN_HEIGHT_COMPACT else CARD_MIN_HEIGHT_WIDE
    TeachSurface(
        modifier = modifier.fillMaxWidth().heightIn(min = minHeight),
        border = BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line),
    ) {
        Crossfade(targetState = revealed, animationSpec = reviewFade(), label = "card-reveal") { back ->
            Column(
                Modifier.fillMaxWidth().padding(cardPadding(width)),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            ) {
                if (back) {
                    CardBack(
                        card,
                        firstOfSession,
                        hardwareKeyboard,
                        onOpenLesson,
                    )
                } else {
                    CardFront(card, position, total, showHint, focus)
                }
            }
        }
    }
}

@Composable
private fun CardCaption(card: ReviewCard) {
    if (card.kind == ReviewCardKind.Term) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeachIcon(
                TeachIcons.BookOpen,
                contentDescription = null,
                tint = TeachTheme.colors.knowledge,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(
                card.topicTitle,
                style = TeachTheme.typography.ui.label,
                color = TeachTheme.colors.ink,
                maxLines = CAPTION_MAX_LINES,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            TeachText(
                stringResource(R.string.review_caption_term_suffix),
                style = TeachTheme.typography.ui.labelS,
                color = TeachTheme.colors.inkSoft,
            )
        }
    } else {
        TeachText(
            card.topicTitle,
            style = TeachTheme.typography.ui.label,
            color = TeachTheme.colors.ink,
            maxLines = CAPTION_MAX_LINES,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Caption and, when due, the «to consolidate» pill, 8 dp apart (review.md section 4.2). */
@Composable
private fun CaptionBlock(card: ReviewCard) {
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        CardCaption(card)
        if (showsConsolidatePill(card)) ConsolidatePill()
    }
}

/** Non-interactive «to consolidate» label (review.md section 4.2); its meaning is announced by the card node. */
@Composable
private fun ConsolidatePill() {
    Row(
        Modifier
            .clearAndSetSemantics { }
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.full)
            .padding(horizontal = TeachTheme.spacing.sm, vertical = TeachTheme.spacing.xxs),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TeachIcon(
            TeachIcons.Repeat,
            contentDescription = null,
            tint = TeachTheme.colors.inkSoft,
            size = TeachTheme.spacing.md,
        )
        TeachText(
            stringResource(R.string.review_consolidate),
            style = TeachTheme.typography.ui.labelS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}

/** The front never shows the lesson title: it could give the answer away (P5, P7). */
@Composable
private fun CardFront(
    card: ReviewCard,
    position: Int,
    total: Int,
    showHint: Boolean,
    focus: FocusRequester,
) {
    val description =
        stringResource(
            frontDescriptionRes(card),
            position,
            total,
            card.topicTitle,
            plainText(card.front),
        )
    Column(
        Modifier
            .focusRequester(focus)
            .focusable()
            .semantics(mergeDescendants = true) { contentDescription = description },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        CaptionBlock(card)
        InlineText(card.front, style = TeachTheme.typography.reading.cardFront, interactive = false)
        if (showHint) {
            TeachText(
                stringResource(R.string.review_hint),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.inkSoft,
                modifier = Modifier.padding(top = TeachTheme.spacing.xs),
            )
        }
    }
}

@Composable
private fun CardBack(
    card: ReviewCard,
    firstOfSession: Boolean,
    hardwareKeyboard: Boolean,
    onOpenLesson: (String) -> Unit,
) {
    val lessonNumber = card.lessonNumber
    val lessonTitle = card.lessonTitle
    val answer = plainText(card.back)
    // The origin is said by the "Apri la lezione" action, not by the answer (review.md section 10.2).
    val description = stringResource(R.string.review_card_back_a11y, answer)
    Column(
        Modifier.semantics(mergeDescendants = true) {
            liveRegion = LiveRegionMode.Polite
            contentDescription = description
        },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        CaptionBlock(card)
        InlineText(
            card.front,
            style = TeachTheme.typography.reading.body,
            color = TeachTheme.colors.inkSoft,
            interactive = false,
            maxLines = FRONT_PREVIEW_LINES,
        )
        Box(Modifier.fillMaxWidth().height(TeachTheme.stroke.hairline).background(TeachTheme.colors.line))
        InlineText(
            card.back,
            style =
                if (isLongBack(
                        card.back,
                    )
                ) {
                    TeachTheme.typography.reading.body
                } else {
                    TeachTheme.typography.reading.cardFront
                },
            interactive = false,
        )
    }
    val origin =
        when {
            lessonNumber != null && !lessonTitle.isNullOrBlank() ->
                stringResource(R.string.review_origin, lessonNumber, plainText(lessonTitle))
            !lessonTitle.isNullOrBlank() -> stringResource(R.string.review_origin_plain, plainText(lessonTitle))
            else -> null
        }
    val lessonId = card.lessonId
    if (lessonId != null) {
        val a11y =
            when {
                lessonNumber != null && !lessonTitle.isNullOrBlank() ->
                    stringResource(
                        if (firstOfSession) {
                            R.string.review_open_lesson_a11y_first
                        } else {
                            R.string.review_open_lesson_a11y
                        },
                        lessonNumber,
                        plainText(lessonTitle),
                    )
                !lessonTitle.isNullOrBlank() ->
                    stringResource(
                        if (firstOfSession) {
                            R.string.review_open_lesson_a11y_plain_first
                        } else {
                            R.string.review_open_lesson_a11y_plain
                        },
                        plainText(lessonTitle),
                    )
                else ->
                    stringResource(
                        if (firstOfSession) {
                            R.string.review_open_lesson_a11y_untitled_first
                        } else {
                            R.string.review_open_lesson
                        },
                    )
            }
        OpenLessonButton(origin, a11y, hardwareKeyboard) { onOpenLesson(lessonId) }
    } else if (origin != null) {
        TeachText(origin, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
    }
}

/** «Apri la lezione» (review.md section 4.6): one text-button target, two lines, 48 dp at least, Alt+L. */
@Composable
private fun OpenLessonButton(
    origin: String?,
    description: String,
    hardwareKeyboard: Boolean,
    onClick: () -> Unit,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.sm
    Row(
        Modifier
            .fillMaxWidth()
            .focusRing(state.focused, shape)
            .background(if (state.pressed) TeachTheme.colors.accentSoft else Color.Transparent, shape)
            .semantics { contentDescription = description }
            .clickable(
                interactionSource = state.source,
                indication = null,
                role = Role.Button,
                onClick = onClick,
            ).heightIn(min = OPEN_LESSON_MIN_HEIGHT)
            .padding(vertical = TeachTheme.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).clearAndSetSemantics { }) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TeachIcon(
                    TeachIcons.BookOpen,
                    contentDescription = null,
                    tint = TeachTheme.colors.accent,
                    size = OPEN_LESSON_ICON,
                )
                TeachText(
                    stringResource(R.string.review_open_lesson),
                    style = TeachTheme.typography.ui.label,
                    color = TeachTheme.colors.accent,
                )
            }
            if (origin != null) {
                TeachText(origin, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
            }
        }
        if (hardwareKeyboard) {
            TeachText(
                stringResource(R.string.review_open_lesson_hint),
                modifier =
                    Modifier
                        .border(BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line), TeachTheme.shapes.sm)
                        .padding(horizontal = TeachTheme.spacing.xs)
                        .clearAndSetSemantics { },
                style = TeachTheme.typography.ui.labelS,
                color = TeachTheme.colors.inkMuted,
            )
        }
    }
}

/** Title of the grades: focus goes here after the reveal. */
@Composable
internal fun RateTitle(focus: FocusRequester) {
    TeachText(
        stringResource(R.string.review_rate_title),
        modifier = Modifier.focusRequester(focus).focusable().semantics { heading() },
        style = TeachTheme.typography.ui.label,
    )
}

@Composable
private fun ratingLabel(rating: ReviewRating): String =
    stringResource(
        when (rating) {
            ReviewRating.Again -> R.string.review_rating_again
            ReviewRating.Hard -> R.string.review_rating_hard
            ReviewRating.Good -> R.string.review_rating_good
            ReviewRating.Easy -> R.string.review_rating_easy
        },
    )

/** The label of a grade, also used by the undo row. */
@Composable
internal fun ratingText(rating: ReviewRating): String = ratingLabel(rating)

/** The four grades, identical in style (P7: no colour, weight or position that favours one). */
@Composable
internal fun RatingButtons(
    card: ReviewCard,
    onRate: (ReviewRating) -> Unit,
    hardwareKeyboard: Boolean,
    modifier: Modifier = Modifier,
) {
    val layout = ratingsLayout(currentWindowWidthClass(), LocalDensity.current.fontScale)
    val button: @Composable (ReviewRating, Modifier) -> Unit = { rating, m ->
        RatingButton(rating, card, onRate, hardwareKeyboard, m)
    }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        when (layout) {
            RatingsLayout.Row ->
                Row(
                    Modifier.height(IntrinsicSize.Max),
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    RATINGS.forEach { button(it, Modifier.weight(1f).fillMaxHeight()) }
                }
            RatingsLayout.Grid ->
                RATINGS.chunked(2).forEach { pair ->
                    Row(
                        Modifier.height(IntrinsicSize.Max),
                        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                    ) {
                        pair.forEach { button(it, Modifier.weight(1f).fillMaxHeight()) }
                    }
                }
            RatingsLayout.Column -> RATINGS.forEach { button(it, Modifier.fillMaxWidth()) }
        }
    }
}

@Composable
private fun RatingButton(
    rating: ReviewRating,
    card: ReviewCard,
    onRate: (ReviewRating) -> Unit,
    hardwareKeyboard: Boolean,
    modifier: Modifier,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val label = ratingLabel(rating)
    val interval = intervalTextOrNull(card.intervals?.forRating(rating))
    val description = if (interval != null) stringResource(R.string.review_rating_a11y, label, interval) else label
    Column(
        modifier =
            modifier
                .focusRing(state.focused, shape)
                .background(if (state.pressed) TeachTheme.colors.surfacePressed else TeachTheme.colors.surface, shape)
                .border(BorderStroke(TeachTheme.stroke.control, TeachTheme.colors.border), shape)
                .semantics { contentDescription = description }
                .clickable(
                    interactionSource = state.source,
                    indication = null,
                    role = Role.Button,
                    onClick = { onRate(rating) },
                ).heightIn(min = RATING_MIN_HEIGHT)
                .padding(TeachTheme.spacing.sm),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Row(
            Modifier.clearAndSetSemantics { },
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.CenterHorizontally),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (hardwareKeyboard) KeyHint(RATINGS.indexOf(rating) + 1)
            TeachText(label, style = TeachTheme.typography.ui.label, textAlign = TextAlign.Center)
        }
        if (interval != null) {
            TeachText(
                interval,
                modifier = Modifier.clearAndSetSemantics { },
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkSoft,
                textAlign = TextAlign.Center,
                overflow = TextOverflow.Clip,
            )
        }
    }
}
