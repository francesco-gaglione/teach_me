package com.teachme.feature.home

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.model.DueTopic

private val CHIP_VISUAL_HEIGHT = 32.dp
private val CHIP_BORDER = 1.5.dp
private const val FONT_SCALE_COLUMN = 2f
private const val CHIP_TITLE_MAX_CHARS = 24

/** One chip: [topicId] null is "Tutti". */
private data class ChipItem(
    val topicId: String?,
    val title: String,
    val count: Int,
)

/**
 * Single-selection filter of the review card (home.md, "Filtro per argomento"): "Tutti" first, then one chip per topic.
 * A scrolling row on normal fonts, a column at font scale 2 so no text is lost and no target scrolls sideways.
 */
@Composable
internal fun ReviewTopicChips(
    topics: List<DueTopic>,
    /** The total of the card, net of the grades still queued: "Tutti" must say the same number as the status line. */
    allCount: Int,
    selectedTopicId: String?,
    onSelect: (String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val all = stringResource(R.string.home_review_all)
    val items =
        listOf(ChipItem(null, all, allCount)) +
            topics.map { ChipItem(it.topicId, it.topicTitle, it.dueCount) }
    val selectedIndex = items.indexOfFirst { it.topicId == selectedTopicId }.coerceAtLeast(0)
    val group = stringResource(R.string.home_review_filter)
    val chipModifier = Modifier
    val groupModifier = modifier.fillMaxWidth().selectableGroup().semantics { contentDescription = group }
    if (LocalDensity.current.fontScale >= FONT_SCALE_COLUMN) {
        Column(groupModifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            items.forEachIndexed { index, item ->
                TopicChip(
                    item,
                    index == selectedIndex,
                    ellipsize = false,
                    chipModifier.fillMaxWidth(),
                ) { onSelect(item.topicId) }
            }
        }
    } else {
        val listState = rememberLazyListState()
        LaunchedEffect(selectedIndex) { listState.animateScrollToItem(selectedIndex) }
        LazyRow(groupModifier, listState, horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            itemsIndexed(items, key = { _, item -> item.topicId ?: "" }) { index, item ->
                TopicChip(item, index == selectedIndex, ellipsize = true, chipModifier) { onSelect(item.topicId) }
            }
        }
    }
}

@Composable
private fun TopicChip(
    item: ChipItem,
    selected: Boolean,
    ellipsize: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val focusManager = LocalFocusManager.current
    val title =
        if (ellipsize &&
            item.title.length > CHIP_TITLE_MAX_CHARS
        ) {
            item.title.take(CHIP_TITLE_MAX_CHARS) + "…"
        } else {
            item.title
        }
    val cards = pluralStringResource(R.plurals.home_review_cards, item.count, item.count)
    // The selected state is announced by `selectable` (Role.RadioButton): not repeated in the text.
    val description = stringResource(R.string.home_review_chip_a11y, item.title, cards)
    val shape = TeachTheme.shapes.full
    // The tappable area is 48 dp tall; the visible pill inside is 32 dp (home.md).
    Row(
        modifier
            .heightIn(min = TeachTheme.layout.minTouchTarget)
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) {
                    false
                } else {
                    when (event.key) {
                        Key.DirectionLeft -> focusManager.moveFocus(FocusDirection.Previous)
                        Key.DirectionRight -> focusManager.moveFocus(FocusDirection.Next)
                        else -> false
                    }
                }
            }.focusRing(state.focused, shape)
            .selectable(
                selected = selected,
                interactionSource = state.source,
                indication = null,
                role = Role.RadioButton,
                onClick = onClick,
            ).semantics { contentDescription = description },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier
                .heightIn(min = CHIP_VISUAL_HEIGHT)
                .background(if (selected) colors.accentSoft else colors.surface, shape)
                .border(CHIP_BORDER, if (selected) colors.accent else colors.border, shape)
                .padding(horizontal = TeachTheme.spacing.sm),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (selected) {
                TeachIcon(TeachIcons.Check, null, tint = colors.ink, size = TeachTheme.layout.iconSizeSmall)
            }
            TeachText(
                stringResource(R.string.home_review_chip, title, item.count),
                style = TeachTheme.typography.ui.label,
                color = colors.ink,
                maxLines = if (ellipsize) 1 else Int.MAX_VALUE,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
