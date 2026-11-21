package com.teachme.feature.lesson

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.sdui.logic.LessonIndex
import com.teachme.sdui.render.PdfSourceTag

/**
 * Index, glossary and sources of the lesson (docs/design/screens/lesson.md section 3.1), used both by the
 * side panel and by the sheet. Glossary and sources list only what the learner has already met, so the
 * panel never gives away the answer of an exercise still to do.
 */
@Composable
fun LessonContextContent(
    ready: LessonUiState.Ready,
    onOpenSection: (Int) -> Unit,
    onOpenTerm: (String) -> Unit,
    onOpenSource: (String) -> Unit,
    onDisclosure: (String, Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val document = ready.document
    val revealed = ready.sections.sections.take(ready.saved.revealedCount)
    val terms = remember(document, revealed) { LessonIndex.encounteredTerms(document, revealed) }
    val sources = remember(document, revealed) { LessonIndex.citedSources(document, revealed) }
    val saved = ready.saved
    Column(modifier.fillMaxWidth()) {
        TeachDisclosure(
            title = stringResource(R.string.lesson_context_index),
            expanded = saved.disclosures[KEY_INDEX] ?: true,
            onToggle = { onDisclosure(KEY_INDEX, !(saved.disclosures[KEY_INDEX] ?: true)) },
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                ready.sections.sections.forEach { section ->
                    SectionRow(
                        title = section.title ?: stringResource(R.string.lesson_section_intro),
                        status =
                            when {
                                section.index < ready.currentIndex -> SectionStatus.Done
                                section.index == ready.currentIndex -> SectionStatus.Current
                                else -> SectionStatus.Todo
                            },
                        onClick = { onOpenSection(section.index) },
                    )
                }
            }
        }
        TeachDisclosure(
            title = stringResource(R.string.lesson_context_glossary, terms.size),
            expanded = saved.disclosures[KEY_GLOSSARY] ?: false,
            onToggle = { onDisclosure(KEY_GLOSSARY, !(saved.disclosures[KEY_GLOSSARY] ?: false)) },
        ) {
            if (terms.isEmpty()) {
                TeachText(
                    stringResource(R.string.lesson_context_glossary_empty),
                    Modifier.padding(TeachTheme.spacing.sm),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
            }
            terms.forEach { key ->
                val entry = document.glossaryEntry(key) ?: return@forEach
                val seen = key in saved.openedTerms
                ContextRow(
                    description =
                        if (seen) "${entry.term}, ${stringResource(R.string.lesson_term_seen)}" else entry.term,
                    onClick = { onOpenTerm(key) },
                ) {
                    TeachText(entry.term, Modifier.weight(1f), style = TeachTheme.typography.ui.body)
                    if (seen) {
                        TeachIcon(
                            TeachIcons.Check,
                            contentDescription = null,
                            tint = TeachTheme.colors.inkSoft,
                            size = TeachTheme.layout.iconSizeSmall,
                        )
                    }
                }
            }
        }
        TeachDisclosure(
            title = stringResource(R.string.lesson_context_sources, sources.size),
            expanded = saved.disclosures[KEY_SOURCES] ?: false,
            onToggle = { onDisclosure(KEY_SOURCES, !(saved.disclosures[KEY_SOURCES] ?: false)) },
        ) {
            if (sources.isEmpty()) {
                TeachText(
                    stringResource(R.string.lesson_context_sources_empty),
                    Modifier.padding(TeachTheme.spacing.sm),
                    style = TeachTheme.typography.ui.body,
                    color = TeachTheme.colors.inkSoft,
                )
            }
            // The PDFs of the learner come first (lesson.md 3.3); the numbers stay those of the text.
            val ordered = sources.sortedBy { if (document.source(it)?.isUserPdf == true) 0 else 1 }
            ordered.forEach { key ->
                val source = document.source(key) ?: return@forEach
                val number = document.sourceNumber(key) ?: return@forEach
                val host =
                    source.url
                        ?.let { runCatching { java.net.URI(it).host }.getOrNull() }
                        ?.removePrefix("www.")
                        .orEmpty()
                val description =
                    if (source.isUserPdf) {
                        stringResource(R.string.lesson_source_pdf_a11y, number, source.title)
                    } else {
                        "${source.title}, $host"
                    }
                ContextRow(
                    description = description,
                    onClick = { onOpenSource(key) },
                    onClickLabel = if (source.isUserPdf) stringResource(R.string.lesson_source_show) else null,
                ) {
                    TeachText(
                        stringResource(R.string.lesson_source_number, number),
                        style = TeachTheme.typography.ui.labelS,
                        color = TeachTheme.colors.knowledge,
                    )
                    Column(Modifier.weight(1f)) {
                        TeachText(source.title, style = TeachTheme.typography.ui.body)
                        if (source.isUserPdf) {
                            PdfSourceTag()
                        } else {
                            TeachText(host, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
                        }
                    }
                }
            }
        }
    }
}

internal const val KEY_INDEX = "index"
internal const val KEY_GLOSSARY = "glossary"
internal const val KEY_SOURCES = "sources"

private enum class SectionStatus { Done, Current, Todo }

@Composable
private fun SectionRow(
    title: String,
    status: SectionStatus,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val statusText =
        stringResource(
            when (status) {
                SectionStatus.Done -> R.string.lesson_section_done
                SectionStatus.Current -> R.string.lesson_section_current
                SectionStatus.Todo -> R.string.lesson_section_todo
            },
        )
    val lockedText = stringResource(R.string.lesson_section_locked_state)
    val enabled = status != SectionStatus.Todo
    Row(
        Modifier
            .fillMaxWidth()
            .focusRing(state.focused && enabled, shape)
            .clip(shape)
            .background(
                when {
                    status == SectionStatus.Current -> colors.accentSoft
                    state.pressed && enabled -> colors.surfacePressed
                    else -> androidx.compose.ui.graphics.Color.Transparent
                },
            ).semantics(mergeDescendants = true) {
                contentDescription = "$title. $statusText"
                if (!enabled) stateDescription = lockedText
            }.then(
                if (enabled) {
                    Modifier.clickable(
                        interactionSource = state.source,
                        indication = null,
                        role = Role.Button,
                        onClick = onClick,
                    )
                } else {
                    Modifier
                },
            ).heightIn(min = TeachTheme.layout.minTouchTarget)
            .padding(horizontal = TeachTheme.spacing.sm, vertical = TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        TeachIcon(
            when (status) {
                SectionStatus.Done -> TeachIcons.Check
                SectionStatus.Current -> TeachIcons.CircleDot
                SectionStatus.Todo -> TeachIcons.Circle
            },
            contentDescription = null,
            tint =
                when (status) {
                    SectionStatus.Done -> colors.inkSoft
                    SectionStatus.Current -> colors.accent
                    SectionStatus.Todo -> colors.border
                },
            size = TeachTheme.layout.iconSizeSmall,
        )
        Column(Modifier.weight(1f)) {
            TeachText(
                title,
                style =
                    if (status ==
                        SectionStatus.Current
                    ) {
                        TeachTheme.typography.ui.label
                    } else {
                        TeachTheme.typography.ui.body
                    },
                color = if (status == SectionStatus.Todo) colors.inkSoft else colors.ink,
            )
            TeachText(statusText, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
        }
    }
}

@Composable
private fun ContextRow(
    description: String,
    onClick: () -> Unit,
    onClickLabel: String? = null,
    content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    Row(
        Modifier
            .fillMaxWidth()
            .focusRing(state.focused, shape)
            .clip(shape)
            .background(
                if (state.pressed) TeachTheme.colors.surfacePressed else androidx.compose.ui.graphics.Color.Transparent,
            ).semantics(mergeDescendants = true) { contentDescription = description }
            .clickable(
                interactionSource = state.source,
                indication = null,
                role = Role.Button,
                onClickLabel = onClickLabel,
                onClick = onClick,
            ).heightIn(min = TeachTheme.layout.minTouchTarget)
            .padding(horizontal = TeachTheme.spacing.sm, vertical = TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        content = content,
    )
}
