package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.selection.rememberSelectionState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R
import com.teachme.sdui.highlight.HighlightTarget
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.CalloutVariant
import com.teachme.sdui.model.GlossaryTermBlock
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.Paragraph

/** `heading` (docs/design/components/heading.md): `ink`, no accent, plain text, Inter. */
@Composable
internal fun HeadingBlock(block: Heading) {
    val env = LocalSdui.current
    val typography = TeachTheme.typography.ui
    // Only the first block is "the title"; any other level 1 heading reads as a level 2 heading.
    val effectiveLevel = if (block.level == 1 && block.id != env.document.titleBlockId) 2 else block.level
    val style =
        when (effectiveLevel) {
            1 -> typography.title1
            2 -> typography.title2
            else -> typography.title3
        }
    TeachText(
        text = block.text,
        modifier = Modifier.fillMaxWidth().semantics { heading() },
        style =
            style.copy(
                lineBreak = LineBreak.Heading,
                hyphens = Hyphens.None,
                localeList = localesFor(effectiveLang(block.lang, env.contentLanguage)),
            ),
        color = TeachTheme.colors.ink,
    )
}

/** `paragraph`: the main vehicle of the explanation, Literata 17/28, selectable. */
@Composable
internal fun ParagraphBlock(block: Paragraph) {
    // The pen takes the text over (highlight.md section 9): touching it with the pen clears a selection.
    val selection = rememberSelectionState()
    SelectionContainer(selection) {
        InlineText(
            markdown = block.text,
            style = TeachTheme.typography.reading.body,
            lang = block.lang,
            highlight = HighlightTarget(block.id) { selection.clear() },
        )
    }
}

/** `callout` (docs/design/components/callout.md): info, tip or warning; never the red of `danger`. */
@Composable
internal fun CalloutBlock(block: Callout) {
    val colors = TeachTheme.colors
    val (accent, soft, icon) =
        when (block.variant) {
            CalloutVariant.Info -> Triple(colors.info, colors.infoSoft, TeachIcons.Info)
            CalloutVariant.Tip -> Triple(colors.tip, colors.tipSoft, TeachIcons.Lightbulb)
            CalloutVariant.Warning -> Triple(colors.warning, colors.warningSoft, TeachIcons.AlertTriangle)
        }
    val defaultTitle =
        stringResource(
            when (block.variant) {
                CalloutVariant.Info -> R.string.sdui_callout_info
                CalloutVariant.Tip -> R.string.sdui_callout_tip
                CalloutVariant.Warning -> R.string.sdui_callout_warning
            },
        )
    val selection = rememberSelectionState()
    SelectionContainer(selection) {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(soft, TeachTheme.shapes.md)
                    .padding(TeachTheme.spacing.md)
                    .semantics(mergeDescendants = true) {},
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachIcon(icon, contentDescription = null, tint = accent)
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachText(
                    text = block.title?.takeIf { it.isNotBlank() } ?: defaultTitle,
                    style = TeachTheme.typography.ui.labelL,
                    color = accent,
                )
                InlineText(
                    markdown = block.text,
                    style = TeachTheme.typography.ui.bodyL,
                    lang = block.lang,
                    highlight = HighlightTarget(block.id) { selection.clear() },
                )
            }
        }
    }
}

/** `glossary_term` (docs/design/components/glossary_term.md): the in-flow presentation of a term. */
@Composable
internal fun GlossaryTermBlockView(block: GlossaryTermBlock) {
    val env = LocalSdui.current
    val entry = remember(block.key, env.document) { env.document.glossaryEntry(block.key) } ?: return
    val colors = TeachTheme.colors
    val avoidLabel = stringResource(R.string.sdui_term_avoid)
    SelectionContainer {
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(colors.knowledgeSoft, TeachTheme.shapes.md)
                    .padding(TeachTheme.spacing.md)
                    .semantics(mergeDescendants = true) {},
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(TeachIcons.BookMarked, contentDescription = null, tint = colors.knowledge)
                TeachText(
                    stringResource(R.string.sdui_term_label),
                    style = TeachTheme.typography.ui.label,
                    color = colors.knowledge,
                )
            }
            TeachText(entry.term, style = TeachTheme.typography.ui.title3, color = colors.ink)
            InlineText(
                markdown = entry.definition,
                style = TeachTheme.typography.reading.body,
                interactive = false,
                lang = block.lang,
            )
            if (entry.avoid.isNotEmpty()) {
                Hairline()
                Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(
                        text = "$avoidLabel ${entry.avoid.joinToString("; ")}",
                        style = TeachTheme.typography.ui.body,
                        color = colors.inkSoft,
                    )
                }
            }
        }
    }
}
