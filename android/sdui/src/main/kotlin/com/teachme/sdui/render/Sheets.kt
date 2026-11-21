package com.teachme.sdui.render

import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachBottomSheet
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.GlossaryEntry
import com.teachme.sdui.model.SourceEntry

/** The definition of a glossary term, in a sheet on every width class (docs/design/components/glossary_term.md). */
@Composable
fun GlossarySheet(
    entry: GlossaryEntry,
    onDismiss: () -> Unit,
) {
    val colors = TeachTheme.colors
    val focus = remember { FocusRequester() }
    LaunchedEffect(entry.key) { runCatching { focus.requestFocus() } }
    TeachBottomSheet(title = entry.term, onDismissRequest = onDismiss) {
        TeachText(
            stringResource(R.string.sdui_term_label),
            style = TeachTheme.typography.ui.label,
            color = colors.knowledge,
        )
        TeachText(
            entry.term,
            Modifier.focusRequester(focus).focusable().semantics { heading() },
            style = TeachTheme.typography.ui.title2,
        )
        InlineText(entry.definition, TeachTheme.typography.reading.body, interactive = false)
        if (entry.avoid.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                TeachText(stringResource(R.string.sdui_term_avoid_sheet), style = TeachTheme.typography.ui.label)
                entry.avoid.forEach {
                    TeachText("• $it", style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                }
            }
        }
        TeachButton(
            stringResource(R.string.sdui_close),
            onDismiss,
            Modifier.fillMaxWidth(),
            variant = ButtonVariant.Secondary,
        )
    }
}

/** A source: title, host, "Apri" (browser) and "Chiudi". */
@Composable
fun SourceSheet(
    number: Int,
    source: SourceEntry,
    onOpen: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = TeachTheme.colors
    val focus = remember { FocusRequester() }
    LaunchedEffect(source.key) { runCatching { focus.requestFocus() } }
    val url = source.url
    if (url == null) {
        PdfSourceSheet(source, onDismiss)
        return
    }
    val opens = url.startsWith("https://", ignoreCase = true)
    TeachBottomSheet(title = source.title, onDismissRequest = onDismiss) {
        TeachText(
            stringResource(R.string.sdui_source_label, number),
            style = TeachTheme.typography.ui.label,
            color = colors.knowledge,
        )
        TeachText(
            source.title,
            Modifier.focusRequester(focus).focusable().semantics { heading() },
            style = TeachTheme.typography.ui.title2,
        )
        TeachText(
            if (opens) displayHost(url).orEmpty() else url,
            style = TeachTheme.typography.ui.body,
            color = colors.inkSoft,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            TeachButton(
                stringResource(R.string.sdui_close),
                onDismiss,
                Modifier.weight(1f),
                variant = ButtonVariant.Secondary,
            )
            if (opens) {
                TeachButton(
                    stringResource(R.string.sdui_source_open),
                    { onOpen(url) },
                    Modifier.weight(1f),
                    size = ButtonSize.Normal,
                )
            }
        }
    }
}

/** Source of the learner's own PDF (lesson.md 3.3): nothing to open, so the only action is "Chiudi". */
@Composable
private fun PdfSourceSheet(
    source: SourceEntry,
    onDismiss: () -> Unit,
) {
    val colors = TeachTheme.colors
    val focus = remember { FocusRequester() }
    LaunchedEffect(source.key) { runCatching { focus.requestFocus() } }
    TeachBottomSheet(title = source.title, onDismissRequest = onDismiss) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(TeachIcons.FileText, contentDescription = null, tint = colors.inkSoft)
            TeachText(
                stringResource(R.string.sdui_pdf_label),
                style = TeachTheme.typography.ui.label,
                color = colors.ink,
            )
        }
        TeachText(
            source.title,
            Modifier.focusRequester(focus).focusable().semantics { heading() },
            style = TeachTheme.typography.ui.title3,
        )
        TeachText(
            stringResource(R.string.sdui_source_pdf_body),
            style = TeachTheme.typography.ui.body,
            color = colors.ink,
        )
        TeachButton(
            stringResource(R.string.sdui_close),
            onDismiss,
            Modifier.fillMaxWidth(),
            variant = ButtonVariant.Secondary,
        )
    }
}

/** "Dal tuo PDF" with the file icon, in place of the domain of a web source (lesson.md 3.3). */
@Composable
fun PdfSourceTag(modifier: Modifier = Modifier) {
    Row(
        modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
    ) {
        TeachIcon(
            TeachIcons.FileText,
            contentDescription = null,
            tint = TeachTheme.colors.inkSoft,
            size = TeachTheme.layout.iconSizeSmall,
        )
        TeachText(
            stringResource(R.string.sdui_pdf_label),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
    }
}
