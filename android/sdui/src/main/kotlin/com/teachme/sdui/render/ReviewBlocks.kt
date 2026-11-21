package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CollectionInfo
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.collectionInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.logic.ExerciseReducers
import com.teachme.sdui.logic.ExerciseState
import com.teachme.sdui.logic.Rating
import com.teachme.sdui.model.Checklist
import com.teachme.sdui.model.Flashcard

private val RATING_MIN_HEIGHT = 64.dp
private val ANSWER_MIN_HEIGHT = 120.dp
private const val ANSWER_MAX_LINES = 10
private const val ANSWER_MIN_LINES = 4
private const val NEAR_LIMIT = 0.9f
private const val LONG_BACK_CHARS = 200
private const val RATING_COLUMNS = 2

/** `flashcard` (docs/design/components/flashcard.md): recall first, honest self-rating, no colors on the grades. */
@Composable
internal fun FlashcardBlock(block: Flashcard) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val locked = env.states.isLocked(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val colors = TeachTheme.colors
    val rate = { r: Rating -> update { ExerciseReducers.rate(it, r) } }
    val ratingsVisible = state.revealed && (state.rating == null || state.ratingOpen)
    val onKey = { e: KeyEvent ->
        val digit = digitOf(e)
        if (digit != null && digit <= Rating.entries.size && ratingsVisible) {
            rate(Rating.entries[digit - 1])
            true
        } else {
            false
        }
    }
    ExerciseCard(
        required = block.required,
        icon = TeachIcons.Layers,
        typeLabel = stringResource(R.string.sdui_type_flashcard),
        onPrimary = null,
        onKey = onKey,
    ) {
        ExerciseCodeSlot(block.code, block.id)
        if (!state.revealed) {
            // With code above, the front steps down to the question size so it does not dominate the snippet.
            val hasCode = block.code != null
            val frontStyle =
                if (hasCode) {
                    TeachTheme.typography.reading.question
                } else {
                    TeachTheme.typography.reading.cardFront
                }
            InlineText(block.front, frontStyle, interactive = false, lang = block.lang)
            TeachText(
                stringResource(R.string.sdui_flashcard_hint),
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
            )
            TeachButton(
                text = stringResource(R.string.sdui_flashcard_show),
                onClick = { update { ExerciseReducers.reveal(it) } },
                variant = ButtonVariant.Secondary,
            )
        } else {
            InlineText(
                block.front,
                TeachTheme.typography.reading.body,
                interactive = false,
                color = colors.inkSoft,
                lang = block.lang,
            )
            Hairline()
            Column(Modifier.semantics { liveRegion = LiveRegionMode.Polite }) {
                SelectionContainer {
                    InlineText(
                        markdown = block.back,
                        style =
                            if (block.back.length >
                                LONG_BACK_CHARS
                            ) {
                                TeachTheme.typography.reading.body
                            } else {
                                TeachTheme.typography.reading.cardFront
                            },
                        lang = block.lang,
                    )
                }
            }
            if (ratingsVisible) {
                TeachText(stringResource(R.string.sdui_flashcard_how), style = TeachTheme.typography.ui.label)
                RatingButtons(env.hardwareKeyboard, rate)
            } else if (state.rating != null) {
                val name = ratingLabel(state.rating)
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachIcon(
                        TeachIcons.Check,
                        contentDescription = null,
                        tint = colors.inkSoft,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                    TeachText(
                        stringResource(R.string.sdui_flashcard_rated, name),
                        Modifier.weight(1f),
                        style = TeachTheme.typography.ui.body,
                        color = colors.inkSoft,
                    )
                    if (!locked) {
                        TeachButton(
                            stringResource(R.string.sdui_flashcard_change),
                            onClick = { update { ExerciseReducers.reopenRating(it) } },
                            variant = ButtonVariant.Text,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ratingLabel(rating: Rating): String =
    stringResource(
        when (rating) {
            Rating.Again -> R.string.sdui_rating_again
            Rating.Hard -> R.string.sdui_rating_hard
            Rating.Good -> R.string.sdui_rating_good
            Rating.Easy -> R.string.sdui_rating_easy
        },
    )

@Composable
private fun RatingButtons(
    hints: Boolean,
    onRate: (Rating) -> Unit,
) {
    val single = isLargeFont()
    val ratings = Rating.entries
    val button: @Composable (Rating, Modifier) -> Unit = { r, m ->
        Row(m, verticalAlignment = Alignment.CenterVertically) {
            TeachButton(
                text = ratingLabel(r),
                onClick = { onRate(r) },
                modifier = Modifier.weight(1f).heightIn(min = RATING_MIN_HEIGHT),
                variant = ButtonVariant.Secondary,
            )
            if (hints) {
                Row(Modifier.padding(start = TeachTheme.spacing.xs)) { KeyHint(r.ordinal + 1) }
            }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        if (single) {
            ratings.forEach { button(it, Modifier.fillMaxWidth()) }
        } else {
            ratings.chunked(RATING_COLUMNS).forEach { rowRatings ->
                Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                    rowRatings.forEach { button(it, Modifier.weight(1f)) }
                }
            }
        }
    }
}

/** `checklist` (docs/design/components/checklist.md): self-declared, never required, no verdict. */
@Composable
internal fun ChecklistBlock(block: Checklist) {
    val env = LocalSdui.current
    val state = env.states.get(block.id)
    val update = { f: (ExerciseState) -> ExerciseState -> env.states.update(block.id, f) }
    val colors = TeachTheme.colors
    val group = rememberGroupFocus(block.items.size)
    val done = block.items.count { it.id in state.selection }
    val title = block.title?.takeIf { it.isNotBlank() } ?: stringResource(R.string.sdui_checklist_default_title)
    val toggle = { id: String -> update { ExerciseReducers.toggleItem(it, id) } }
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.surface, TeachTheme.shapes.md)
            .border(TeachTheme.stroke.hairline, colors.line, TeachTheme.shapes.md)
            .padding(TeachTheme.spacing.md)
            .onPreviewKeyEvent { e ->
                if (e.type == KeyEventType.KeyDown &&
                    (e.key == Key.DirectionDown || e.key == Key.DirectionUp || digitOf(e) != null)
                ) {
                    choiceKeys(e, group, block.items.size, false, onPick = { i ->
                        toggle(block.items[i].id)
                    }, moveSelects = false, onEnter = {})
                } else {
                    false
                }
            },
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(
                TeachIcons.ListChecks,
                contentDescription = null,
                tint = colors.inkSoft,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(title, Modifier.semantics { heading() }, style = TeachTheme.typography.ui.title3)
        }
        Column(
            Modifier.selectableGroup().semantics { collectionInfo = CollectionInfo(block.items.size, 1) },
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            block.items.forEachIndexed { i, item ->
                ChoiceRow(
                    data = ChoiceRowData(item.text, item.id in state.selection, ChoiceMark.None, null, null, false),
                    multi = true,
                    index = i,
                    count = block.items.size,
                    locked = false,
                    onClick = {
                        toggle(item.id)
                        runCatching { group.requesters[i].requestFocus() }
                    },
                    onToggleWhy = {},
                    keyHint = if (env.hardwareKeyboard) i + 1 else null,
                    lang = block.lang,
                    rowModifier = Modifier.groupRow(group, i),
                )
            }
        }
        TeachText(
            pluralStringResource(R.plurals.sdui_checklist_count, block.items.size, done, block.items.size),
            Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            style = TeachTheme.typography.ui.bodyS,
            color = colors.inkSoft,
        )
    }
}
