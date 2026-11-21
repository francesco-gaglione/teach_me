package com.teachme.feature.lesson

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachBottomSheet
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.RecordKind
import com.teachme.core.model.RecordStatus
import com.teachme.sdui.inline.InlineMarkdown
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.render.ProvideStandaloneSdui
import java.time.Instant

/** Where a record or term came from: the lesson number and title when the list of lessons knows it. */
data class LessonOrigin(
    val lessonId: String,
    val number: Int?,
    val title: String?,
)

/** Eyebrow icon and text per record kind (progress.md section 3.2): competence, never blame. */
@Composable
internal fun recordEyebrow(
    kind: RecordKind,
    superseded: Boolean,
): Pair<Int, String> =
    when {
        superseded -> TeachIcons.History to stringResource(R.string.progress_record_superseded)
        kind == RecordKind.MisconceptionCorrected ->
            TeachIcons.Lightbulb to
                stringResource(R.string.progress_record_corrected)
        kind == RecordKind.Demonstrated -> TeachIcons.Check to stringResource(R.string.progress_record_demonstrated)
        else -> TeachIcons.Check to stringResource(R.string.progress_record_registered)
    }

/**
 * A learning record (progress.md section 3.2). With [framed] false it has no card of its own (inside the summary block,
 * where the separation is a hairline). [origin] adds the "Lezione N" button that opens that lesson.
 */
@Composable
fun RecordCard(
    record: LearningRecord,
    modifier: Modifier = Modifier,
    framed: Boolean = true,
    origin: LessonOrigin? = null,
    onOpenLesson: ((String) -> Unit)? = null,
    highlighted: Boolean = false,
    successorAvailable: Boolean = false,
    onSeeSuccessor: (() -> Unit)? = null,
) {
    val superseded = record.status == RecordStatus.Superseded
    val colors = TeachTheme.colors
    val (icon, eyebrow) = recordEyebrow(record.kind, superseded)
    var evidenceOpen by rememberSaveable(record.id) { mutableStateOf(false) }
    val titleColor = if (superseded) colors.inkSoft else colors.ink
    val lessonLabel =
        origin?.let {
            if (it.number !=
                null
            ) {
                stringResource(R.string.progress_lesson_n, it.number)
            } else {
                stringResource(R.string.progress_lesson)
            }
        }
    val frame =
        if (framed) {
            modifier
                .fillMaxWidth()
                .background(colors.surface, TeachTheme.shapes.md)
                .border(
                    BorderStroke(
                        if (highlighted) TeachTheme.stroke.selected else TeachTheme.stroke.hairline,
                        if (highlighted) colors.accent else colors.line,
                    ),
                    TeachTheme.shapes.md,
                ).padding(TeachTheme.spacing.md)
        } else {
            modifier.fillMaxWidth()
        }
    val originA11y = origin?.number?.let { stringResource(R.string.progress_record_from_lesson_a11y, it) }
    val a11y =
        buildString {
            append(eyebrow).append(". ").append(InlineMarkdown.plainText(record.title) { null }).append(". ")
            append(InlineMarkdown.plainText(record.body) { null }).append('.')
            if (originA11y != null) append(' ').append(originA11y)
        }
    ProvideStandaloneSdui {
        Column(frame, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Row(
                    Modifier.weight(1f).semantics(mergeDescendants = true) { contentDescription = a11y },
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                ) {
                    TeachIcon(icon, null, tint = colors.inkSoft, size = TeachTheme.layout.iconSizeSmall)
                    TeachText(
                        eyebrow,
                        Modifier.weight(1f),
                        style = TeachTheme.typography.ui.labelS,
                        color = colors.inkSoft,
                    )
                }
                if (lessonLabel != null && onOpenLesson != null) {
                    TeachButton(
                        lessonLabel,
                        { onOpenLesson(origin.lessonId) },
                        variant = ButtonVariant.Text,
                        contentDescription = stringResource(R.string.progress_open_lesson_a11y, lessonLabel),
                    )
                }
            }
            InlineText(
                record.title,
                TeachTheme.typography.ui.title3,
                color = titleColor,
                interactive = false,
            )
            InlineText(
                record.body,
                TeachTheme.typography.reading.body,
                color = titleColor,
                interactive = false,
            )
            if (superseded && successorAvailable && onSeeSuccessor != null) {
                TeachButton(
                    stringResource(R.string.progress_see_successor),
                    onSeeSuccessor,
                    variant = ButtonVariant.Text,
                )
            } else if (superseded) {
                TeachText(
                    stringResource(R.string.progress_superseded_plain),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
            }
            TeachDisclosure(
                title = stringResource(R.string.progress_evidence),
                expanded = evidenceOpen,
                onToggle = { evidenceOpen = !evidenceOpen },
            ) {
                InlineText(
                    record.evidence,
                    TeachTheme.typography.ui.body,
                    color = colors.inkSoft,
                    interactive = false,
                )
            }
        }
    }
}

/** A new or revised glossary term in the summary (lesson.md section 7.2): a 48 dp pill that opens its sheet. */
@Composable
fun TermPill(
    term: GlossaryTerm,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.full
    Row(
        modifier
            .heightIn(min = TeachTheme.layout.minTouchTarget)
            .focusRing(state.focused, shape)
            .background(if (state.pressed) colors.surfacePressed else colors.surface, shape)
            .border(TeachTheme.stroke.control, colors.border, shape)
            .semantics(mergeDescendants = true) { contentDescription = term.term }
            .clickable(interactionSource = state.source, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = TeachTheme.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        TeachIcon(TeachIcons.BookOpen, null, tint = colors.knowledge, size = TeachTheme.layout.iconSizeSmall)
        TeachText(term.term, style = TeachTheme.typography.ui.label, color = colors.ink)
    }
}

/** The terms as pills, wrapping to as many rows as needed (one per row at very large fonts). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TermPills(
    terms: List<GlossaryTerm>,
    onOpen: (GlossaryTerm) -> Unit,
    modifier: Modifier = Modifier,
) {
    FlowRow(
        modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
    ) {
        terms.forEach { TermPill(it, onClick = { onOpen(it) }) }
    }
}

/**
 * The sheet of a glossary term of the Topic (progress.md section 4.2): the definition, "Evita:" with the words not to use,
 * and where it was introduced, with a button to open that lesson.
 */
@Composable
fun TopicGlossarySheet(
    term: GlossaryTerm,
    /** The lesson that last touched the term (`lesson_id`): target of the button. */
    origin: LessonOrigin?,
    /** Resolves any lesson id of the Topic to its number, null when unknown. */
    numberOf: (String) -> Int?,
    onOpenLesson: ((String) -> Unit)?,
    onDismiss: () -> Unit,
) {
    val colors = TeachTheme.colors
    val focus = remember { FocusRequester() }
    LaunchedEffect(term.key) { runCatching { focus.requestFocus() } }
    TeachBottomSheet(title = term.term, onDismissRequest = onDismiss) {
        ProvideStandaloneSdui {
            TeachText(
                stringResource(R.string.progress_term_label),
                style = TeachTheme.typography.ui.label,
                color = colors.knowledge,
            )
            TeachText(
                term.term,
                Modifier.focusRequester(focus).focusable().semantics { heading() },
                style = TeachTheme.typography.ui.title2,
            )
            InlineText(term.definition, TeachTheme.typography.reading.body, interactive = false)
            if (term.avoid.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
                    TeachText(stringResource(R.string.progress_term_avoid), style = TeachTheme.typography.ui.label)
                    term.avoid.forEach {
                        TeachText("• $it", style = TeachTheme.typography.ui.body, color = colors.inkSoft)
                    }
                }
            }
            when (val line = termOriginLine(term.firstLessonId, term.lessonId, numberOf)) {
                is TermOriginLine.Introduced ->
                    stringResource(R.string.progress_term_introduced, line.first)
                is TermOriginLine.IntroducedRevised ->
                    stringResource(R.string.progress_term_introduced_revised, line.first, line.last)
                is TermOriginLine.Revised -> stringResource(R.string.progress_term_revised, line.last)
                null -> null
            }?.let { TeachText(it, style = TeachTheme.typography.ui.body, color = colors.inkSoft) }
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (shouldOfferOpenLesson(origin, onOpenLesson != null) && origin != null && onOpenLesson != null) {
                // With the number unresolved the button stays, with the generic label (progress.md 4.2).
                val lastNumber = origin.number
                val label =
                    if (lastNumber != null) {
                        stringResource(R.string.progress_open_lesson_n, lastNumber)
                    } else {
                        stringResource(R.string.progress_open_lesson)
                    }
                TeachButton(
                    label,
                    { onOpenLesson(origin.lessonId) },
                    variant = ButtonVariant.Text,
                    contentDescription =
                        origin.title?.takeIf { it.isNotBlank() }?.let {
                            if (lastNumber != null) {
                                stringResource(R.string.progress_open_lesson_n_a11y, lastNumber, it)
                            } else {
                                stringResource(R.string.progress_open_lesson_a11y, it)
                            }
                        } ?: label,
                )
            }
            TeachButton(
                stringResource(R.string.lesson_close_sheet),
                onDismiss,
                Modifier.weight(1f),
                variant = ButtonVariant.Secondary,
            )
        }
    }
}

private val previewRecord =
    LearningRecord(
        id = "r1",
        number = 4,
        kind = RecordKind.MisconceptionCorrected,
        title = "Dopo un move la variabile di partenza non è più utilizzabile",
        body = "Pensavi che `a` restasse valida dopo `let b = a;`. Ora prevedi l'errore del compilatore.",
        evidence = "Hai sbagliato la domanda sul move e poi hai spiegato perché `a` non è più valida.",
        status = RecordStatus.Active,
        supersededBy = null,
        lessonId = "l2",
        createdAt = Instant.EPOCH,
    )

private val previewTerm =
    GlossaryTerm(
        "t1",
        "ownership",
        "ownership",
        "La regola per cui ogni valore ha un solo proprietario.",
        listOf("proprietà"),
        "l2",
        Instant.EPOCH,
        Instant.EPOCH,
    )

@Composable
private fun CardSamples() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        RecordCard(previewRecord, origin = LessonOrigin("l2", 2, "Ownership"), onOpenLesson = {})
        RecordCard(previewRecord.copy(kind = RecordKind.Demonstrated, title = "Sai usare `&` per prestare un valore"))
        RecordCard(
            previewRecord.copy(
                status = RecordStatus.Superseded,
                supersededBy = "r9",
            ),
            successorAvailable = true,
            onSeeSuccessor = {
            },
        )
        TermPills(listOf(previewTerm, previewTerm.copy(id = "t2", term = "borrowing")), {})
    }
}

@Preview(name = "Record cards light", showBackground = true, widthDp = 420, heightDp = 900)
@Composable
private fun RecordLightPreview() = TeachTheme(darkTheme = false) { CardSamples() }

@Preview(name = "Record cards dark", showBackground = true, widthDp = 420, heightDp = 900)
@Composable
private fun RecordDarkPreview() = TeachTheme(darkTheme = true) { CardSamples() }

@Preview(name = "Record cards font 2.0", showBackground = true, widthDp = 360, heightDp = 1600, fontScale = 2f)
@Composable
private fun RecordLargePreview() = TeachTheme(darkTheme = false) { CardSamples() }
