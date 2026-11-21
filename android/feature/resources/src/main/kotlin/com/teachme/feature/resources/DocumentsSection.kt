package com.teachme.feature.resources

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.DialogErrorPanel
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachConfirmDialog
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListCard
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.UploadSource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val ROW_MIN_HEIGHT = 96.dp
private val INDEX_ROW_MIN_HEIGHT = 40.dp
private val INDEX_INDENT = 16.dp
private val PROGRESS_BAR_HEIGHT = 4.dp
private const val BAR_SEGMENT_FRACTION = 0.35f
private const val BAR_CYCLE_MS = 1_400
private const val LARGE_FONT_SCALE = 1.3f
private const val PDF_MIME = "application/pdf"

/** Everything the "I tuoi documenti" section can ask of its owner. */
@Immutable
class DocumentsActions(
    val onUpload: (UploadSource) -> Unit,
    val onRetryLoad: () -> Unit,
    val onRetryUpload: () -> Unit,
    /** "Annulla" while sending, "Chiudi" on the upload error. */
    val onCancelUpload: () -> Unit,
    val onDelete: (String) -> Unit,
    val onClearDeleteError: () -> Unit,
    val onRetryImport: (String) -> Unit,
    val onDismissBanner: (List<String>) -> Unit,
    val onOpenSettings: () -> Unit,
)

/**
 * "I tuoi documenti" (resources.md section 13): the PDFs of the Topic, always at the top of the Fonti tab.
 *
 * @param missionPending true before the mission is confirmed (state A): adds the sentence about the web search.
 * @param bannerDocuments the documents the banner "Ho letto il tuo PDF" would name, empty when it must not be shown here.
 */
@Composable
internal fun DocumentsSection(
    state: DocumentsUiState,
    missionPending: Boolean,
    bannerDocuments: List<TopicDocument>,
    acting: Boolean,
    onRefreshSources: () -> Unit,
    actions: DocumentsActions,
    modifier: Modifier = Modifier,
) {
    val count = state.documents.size
    var pendingDelete by rememberSaveable { mutableStateOf<String?>(null) }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
            TeachText(
                if (count >= 1) stringResource(R.string.doc_title_count, count) else stringResource(R.string.doc_title),
                Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.title3,
            )
            if (count >= 1) {
                TeachText(
                    stringResource(R.string.doc_subtitle),
                    style = TeachTheme.typography.ui.bodyS,
                    color = TeachTheme.colors.inkSoft,
                )
            }
        }
        when (val load = state.load) {
            DocumentsLoad.Loading -> DocumentsSkeleton()
            is DocumentsLoad.Failed -> DocumentsLoadError(load.error, actions)
            DocumentsLoad.Ready ->
                DocumentsReady(
                    state,
                    missionPending,
                    bannerDocuments,
                    acting,
                    onRefreshSources,
                    actions,
                    onAskDelete = { pendingDelete = it },
                )
        }
    }
    DocumentAnnouncer(state.announcement)
    val target = state.documents.firstOrNull { it.id == pendingDelete }
    LaunchedEffect(target) { if (target == null && pendingDelete != null) pendingDelete = null }
    if (target != null) {
        val deleting = (state.delete as? DeleteUi.Deleting)?.id == target.id
        val failed = (state.delete as? DeleteUi.Failed)?.takeIf { it.id == target.id }
        TeachConfirmDialog(
            title = stringResource(R.string.doc_delete_title),
            body = stringResource(R.string.doc_delete_body, target.listTitle()),
            confirmLabel = stringResource(R.string.doc_remove),
            dismissLabel = stringResource(R.string.doc_delete_cancel),
            onConfirm = { actions.onDelete(target.id) },
            onDismiss = {
                actions.onClearDeleteError()
                pendingDelete = null
            },
            destructive = true,
            loading = deleting,
            loadingLabel = stringResource(R.string.doc_removing),
            error =
                failed?.let {
                    DialogErrorPanel(
                        stringResource(R.string.doc_delete_error_title),
                        stringResource(
                            when (it.error) {
                                DeleteError.Network -> R.string.doc_delete_error_network
                                DeleteError.Busy -> R.string.doc_delete_error_busy
                                DeleteError.Other -> R.string.doc_delete_error_other
                            },
                        ),
                    )
                },
        )
    }
}

@Composable
private fun DocumentsReady(
    state: DocumentsUiState,
    missionPending: Boolean,
    bannerDocuments: List<TopicDocument>,
    acting: Boolean,
    onRefreshSources: () -> Unit,
    actions: DocumentsActions,
    onAskDelete: (String) -> Unit,
) {
    val sending = state.upload as? UploadUi.Sending
    val availability = uploadAvailability(state.documents, uploading = sending != null)
    val picker = rememberPdfPicker(actions.onUpload)
    val empty = state.documents.isEmpty() && sending == null
    if (empty) {
        EmptyDocuments(picker.launch)
    } else {
        TeachListCard {
            Column {
                sending?.let { UploadingRow(it.name, actions.onCancelUpload) }
                state.documents.forEachIndexed { index, document ->
                    if (index > 0 || sending != null) TeachRowDivider()
                    DocumentRow(document, state, actions, onAskDelete)
                }
            }
        }
    }
    (state.upload as? UploadUi.Failed)?.let { UploadErrorPanel(it.error, actions) }
    if (!empty) {
        when (availability) {
            UploadAvailability.Available -> UploadButton(picker.launch)
            UploadAvailability.LimitReached -> Hint(stringResource(R.string.doc_limit_reached))
            UploadAvailability.ImportBusy -> Hint(stringResource(R.string.doc_busy))
        }
    }
    if (missionPending && state.documents.isNotEmpty()) Hint(stringResource(R.string.doc_pending_mission))
    if (bannerDocuments.isNotEmpty()) {
        val ids = bannerDocuments.map { it.id }
        TeachStatusPanel(
            StatusVariant.Info,
            pluralStringResource(R.plurals.doc_banner_title, bannerDocuments.size),
            body = stringResource(R.string.doc_banner_body),
            announce = false,
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachButton(
                    stringResource(R.string.res_refresh),
                    {
                        actions.onDismissBanner(ids)
                        onRefreshSources()
                    },
                    variant = ButtonVariant.Primary,
                    loading = acting,
                )
                TeachButton(
                    stringResource(R.string.doc_banner_not_now),
                    { actions.onDismissBanner(ids) },
                    variant = ButtonVariant.Text,
                )
            }
        }
    }
}

@Composable
private fun Hint(text: String) =
    TeachText(text, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)

@Composable
private fun UploadButton(onClick: () -> Unit) =
    TeachButton(
        stringResource(R.string.doc_upload),
        onClick,
        variant = ButtonVariant.Secondary,
        leadingIcon = TeachIcons.FileUp,
    )

/** The system file picker for PDFs (`ACTION_OPEN_DOCUMENT`, no storage permission). Cancelling it does nothing. */
private class PdfPicker(
    val launch: () -> Unit,
)

@Composable
private fun rememberPdfPicker(onPicked: (UploadSource) -> Unit): PdfPicker {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val resolver = context.applicationContext.contentResolver
    val launcher =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) {
                scope.launch {
                    // Name and size come from the provider, which may be slow (a cloud drive): off the main thread.
                    val source =
                        withContext(Dispatchers.IO) {
                            // Keeps the file readable for "Riprova", even if the screen is recreated.
                            runCatching {
                                resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
                            }
                            ContentUriSource.resolve(resolver, uri)
                        }
                    onPicked(source)
                }
            }
        }
    return remember(launcher) { PdfPicker { launcher.launch(arrayOf(PDF_MIME)) } }
}

@Composable
private fun EmptyDocuments(onUpload: () -> Unit) {
    val padding = if (currentWindowWidthClass() == WindowWidthClass.Compact) TeachTheme.spacing.md else 20.dp
    TeachSurface(
        Modifier.fillMaxWidth(),
        border = BorderStroke(TeachTheme.stroke.hairline, TeachTheme.colors.line),
    ) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = padding, vertical = TeachTheme.spacing.md),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(TeachIcons.FileText, contentDescription = null, tint = TeachTheme.colors.inkSoft)
                TeachText(stringResource(R.string.doc_empty_title), style = TeachTheme.typography.ui.title3)
            }
            TeachText(
                stringResource(R.string.doc_empty_body),
                style = TeachTheme.typography.ui.body,
                color = TeachTheme.colors.ink,
            )
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                Hint(stringResource(R.string.doc_empty_note_text))
                Hint(stringResource(R.string.doc_empty_note_limits))
                Hint(stringResource(R.string.doc_empty_note_privacy))
            }
            UploadButton(onUpload)
        }
    }
}

@Composable
private fun DocumentsSkeleton() {
    val loading = stringResource(R.string.doc_loading)
    Box(
        Modifier
            .fillMaxWidth()
            .height(ROW_MIN_HEIGHT)
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md)
            .semantics(mergeDescendants = true) { stateDescription = loading },
    )
}

@Composable
private fun DocumentsLoadError(
    error: LoadError,
    actions: DocumentsActions,
) {
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(R.string.doc_load_failed_title),
        body =
            stringResource(
                when (error) {
                    LoadError.Network, LoadError.Unauthorized -> R.string.res_failed_network
                    LoadError.Server -> R.string.res_failed_server
                },
            ),
    ) {
        TeachButton(
            stringResource(R.string.doc_retry),
            actions.onRetryLoad,
            variant = ButtonVariant.Secondary,
            leadingIcon = TeachIcons.RotateCcw,
        )
    }
}

@Composable
private fun UploadErrorPanel(
    error: UploadError,
    actions: DocumentsActions,
) {
    val (title, body) =
        when (error) {
            UploadError.TooLarge -> R.string.doc_upload_too_large_title to R.string.doc_upload_too_large_body
            UploadError.NotPdf -> R.string.doc_upload_not_pdf_title to R.string.doc_upload_not_pdf_body
            UploadError.Network -> R.string.doc_upload_network_title to R.string.doc_upload_network_body
        }
    // Live region: the panel appears after a tap on the picker's result, and its text is what the learner needs.
    TeachStatusPanel(StatusVariant.Danger, stringResource(title), body = stringResource(body)) {
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            if (error == UploadError.Network) {
                TeachButton(
                    stringResource(R.string.doc_retry),
                    actions.onRetryUpload,
                    variant = ButtonVariant.Secondary,
                    leadingIcon = TeachIcons.RotateCcw,
                )
            }
            TeachButton(stringResource(R.string.doc_close), actions.onCancelUpload, variant = ButtonVariant.Text)
        }
    }
}

// Rows

@Composable
private fun DocumentRow(
    document: TopicDocument,
    state: DocumentsUiState,
    actions: DocumentsActions,
    onAskDelete: (String) -> Unit,
) {
    when (document.status) {
        DocumentStatus.Ready -> ReadyRow(document, state.topicLanguage, onAskDelete)
        DocumentStatus.Importing ->
            ImportingRow(document.listTitle(), importPhase(jobFor(document, state.jobs, state.importJob)))
        DocumentStatus.Failed -> FailedRow(document, state, actions)
    }
}

/** A tap target of 48 x 48 dp with the trash icon (resources.md 13.4). */
@Composable
private fun RemoveButton(
    title: String,
    onClick: () -> Unit,
) = TeachIconButton(
    TeachIcons.Trash,
    stringResource(R.string.doc_remove_a11y, title),
    onClick,
)

@Composable
private fun ReadyRow(
    document: TopicDocument,
    topicLanguage: TopicLanguage,
    onAskDelete: (String) -> Unit,
) {
    val colors = TeachTheme.colors
    val pages = document.pageCount?.let { pluralStringResource(R.plurals.doc_pages, it, it) }
    val language = languageNameOrNull(document.language, topicLanguage)
    val meta =
        listOfNotNull(pages, language?.let { stringResource(R.string.doc_in_language, it) })
            .joinToString(" · ")
            .ifEmpty { null }
    val title = document.listTitle()
    val description =
        if (meta != null) {
            stringResource(R.string.doc_row_description_meta, title, meta)
        } else {
            stringResource(R.string.doc_row_description, title)
        }
    var expanded by rememberSaveable(document.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().heightIn(min = ROW_MIN_HEIGHT).padding(vertical = TeachTheme.spacing.xs)) {
        Row(
            Modifier.padding(start = TeachTheme.spacing.md),
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachIcon(
                TeachIcons.FileText,
                contentDescription = null,
                tint = colors.inkSoft,
                modifier = Modifier.padding(top = TeachTheme.spacing.sm),
            )
            Column(
                Modifier
                    .weight(1f)
                    .padding(vertical = TeachTheme.spacing.sm)
                    .semantics(mergeDescendants = true) { contentDescription = description },
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs),
            ) {
                TeachText(title, style = TeachTheme.typography.ui.title3, color = colors.ink)
                if (meta != null) TeachText(meta, style = TeachTheme.typography.ui.body, color = colors.ink)
                if (document.showsFilename()) {
                    TeachText(document.filename, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
                }
            }
            RemoveButton(title) { onAskDelete(document.id) }
        }
        if (document.hasIndex()) {
            val indexTitle =
                pluralStringResource(R.plurals.doc_index_title, document.sections.size, document.sections.size)
            TeachDisclosure(
                title = indexTitle,
                expanded = expanded,
                onToggle = { expanded = !expanded },
                modifier = Modifier.padding(horizontal = TeachTheme.spacing.md),
            ) {
                DocumentIndex(document)
            }
        }
    }
}

/** The table of contents, read-only (13.4): no checkboxes, nothing to tap. */
@Composable
private fun DocumentIndex(document: TopicDocument) {
    val colors = TeachTheme.colors
    val stacked = LocalDensity.current.fontScale > LARGE_FONT_SCALE
    Column(Modifier.fillMaxWidth().padding(bottom = TeachTheme.spacing.xs)) {
        TeachText(
            stringResource(R.string.doc_index_hint),
            Modifier.padding(bottom = TeachTheme.spacing.xs),
            style = TeachTheme.typography.ui.bodyS,
            color = colors.inkSoft,
        )
        document.sections.forEach { section ->
            val range = section.pageRangeOrNull()
            val pages =
                range?.let {
                    if (it.single) {
                        stringResource(R.string.doc_pages_single, it.start)
                    } else {
                        stringResource(R.string.doc_pages_range, it.start, it.end)
                    }
                }
            val indent = INDEX_INDENT * (section.level - 1).coerceIn(0, 2)
            val pagesStyle =
                TeachTheme.typography.ui.bodyS
                    .copy(fontFeatureSettings = "tnum")
            if (stacked || pages == null) {
                Column(
                    Modifier.fillMaxWidth().heightIn(min = INDEX_ROW_MIN_HEIGHT).padding(start = indent),
                    verticalArrangement = Arrangement.Center,
                ) {
                    TeachText(section.title, style = TeachTheme.typography.ui.body, color = colors.ink)
                    if (pages != null) TeachText(pages, style = pagesStyle, color = colors.inkSoft)
                }
            } else {
                Row(
                    Modifier.fillMaxWidth().heightIn(min = INDEX_ROW_MIN_HEIGHT).padding(start = indent),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                ) {
                    TeachText(
                        section.title,
                        Modifier.weight(1f),
                        style = TeachTheme.typography.ui.body,
                        color = colors.ink,
                    )
                    TeachText(pages, style = pagesStyle, color = colors.inkSoft)
                }
            }
        }
    }
}

@Composable
private fun phaseText(phase: ImportPhase): String =
    when (phase) {
        ImportPhase.Uploading -> stringResource(R.string.doc_phase_uploading)
        ImportPhase.Queued -> stringResource(R.string.doc_phase_queued)
        ImportPhase.Reading -> stringResource(R.string.doc_phase_reading)
        is ImportPhase.Extracting ->
            if (phase.done != null && phase.total != null) {
                stringResource(R.string.doc_phase_extracting_pages, phase.done, phase.total)
            } else {
                stringResource(R.string.doc_phase_extracting)
            }
        ImportPhase.Sectioning -> stringResource(R.string.doc_phase_sectioning)
        ImportPhase.Saving -> stringResource(R.string.doc_phase_saving)
        ImportPhase.Unknown -> stringResource(R.string.doc_phase_unknown)
    }

/** What a screen reader hears at a change of phase: the phase, never the page counter. */
@Composable
private fun phaseAnnouncement(phase: ImportPhase): String =
    if (phase is ImportPhase.Extracting) stringResource(R.string.doc_phase_extracting) else phaseText(phase)

@Composable
private fun UploadingRow(
    name: String,
    onCancel: () -> Unit,
) = ProgressRow(name, ImportPhase.Uploading, onCancel)

@Composable
private fun ImportingRow(
    title: String,
    phase: ImportPhase,
) = ProgressRow(title, phase, onCancel = null)

/** A document being sent or read: one line of state, an indeterminate bar that never fills (13.4). */
@Composable
private fun ProgressRow(
    title: String,
    phase: ImportPhase,
    onCancel: (() -> Unit)?,
) {
    val colors = TeachTheme.colors
    val text = phaseText(phase)
    val description = stringResource(R.string.doc_importing_description, title)
    val indent = TeachTheme.layout.iconSize + TeachTheme.spacing.sm
    Box(Modifier.fillMaxWidth()) {
        Column(
            Modifier.fillMaxWidth().heightIn(min = ROW_MIN_HEIGHT).padding(TeachTheme.spacing.md),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            // One reading node for the state; the cancel button below stays a separate target.
            Column(
                Modifier.semantics(mergeDescendants = true) {
                    contentDescription = description
                    stateDescription = text
                },
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
                    TeachIcon(TeachIcons.FileText, contentDescription = null, tint = colors.inkSoft)
                    TeachText(title, Modifier.weight(1f), style = TeachTheme.typography.ui.title3, color = colors.ink)
                }
                TeachText(
                    text,
                    Modifier.padding(start = indent),
                    style = TeachTheme.typography.ui.body,
                    color = colors.ink,
                )
                IndeterminateBar(Modifier.padding(start = indent))
                if (onCancel == null) {
                    TeachText(
                        stringResource(R.string.doc_progress_footer),
                        Modifier.padding(start = indent),
                        style = TeachTheme.typography.ui.bodyS,
                        color = colors.inkSoft,
                    )
                }
            }
            if (onCancel != null) {
                TeachButton(stringResource(R.string.doc_cancel), onCancel, variant = ButtonVariant.Text)
            }
        }
        // Outside the merged node: speaks only when the phase changes, so the page counter is never read out.
        key(phase.announcedKey) {
            val announcement = phaseAnnouncement(phase)
            Box(
                Modifier.size(1.dp).semantics {
                    liveRegion = LiveRegionMode.Polite
                    contentDescription = announcement
                },
            )
        }
    }
}

/** A moving segment on a track: it says "working" and nothing about how long it takes. Static if motion is reduced. */
@Composable
private fun IndeterminateBar(modifier: Modifier = Modifier) {
    if (TeachTheme.reducedMotion) return
    val track = TeachTheme.colors.line
    val segment = TeachTheme.colors.accent
    val progress by
        rememberInfiniteTransition(label = "import-bar").animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(BAR_CYCLE_MS, easing = LinearEasing), RepeatMode.Restart),
            label = "import-bar-offset",
        )
    Canvas(modifier.fillMaxWidth().height(PROGRESS_BAR_HEIGHT).clearAndSetSemantics { }) {
        val radius = size.height / 2
        drawRoundRect(
            track,
            size = size,
            cornerRadius =
                androidx.compose.ui.geometry
                    .CornerRadius(radius),
        )
        val width = size.width * BAR_SEGMENT_FRACTION
        val x = (size.width + width) * progress - width
        val start = x.coerceAtLeast(0f)
        val end = (x + width).coerceAtMost(size.width)
        if (end > start) {
            drawRoundRect(
                segment,
                topLeft = Offset(start, 0f),
                size = Size(end - start, size.height),
                cornerRadius =
                    androidx.compose.ui.geometry
                        .CornerRadius(radius),
            )
        }
    }
}

@Composable
private fun FailedRow(
    document: TopicDocument,
    state: DocumentsUiState,
    actions: DocumentsActions,
) {
    val colors = TeachTheme.colors
    val failure = DocumentFailure.fromCode(document.error?.code)
    val retry = retryableJob(document, state.documents, jobFor(document, state.jobs, state.importJob))
    val removing = (state.delete as? DeleteUi.Deleting)?.id == document.id
    val deleteFailed = (state.delete as? DeleteUi.Failed)?.takeIf { it.id == document.id }
    val title =
        stringResource(
            when (failure) {
                DocumentFailure.NoText -> R.string.doc_failed_no_text_title
                DocumentFailure.Encrypted -> R.string.doc_failed_encrypted_title
                DocumentFailure.TooManyPages -> R.string.doc_failed_pages_title
                DocumentFailure.InvalidPdf -> R.string.doc_failed_invalid_title
                DocumentFailure.Other -> R.string.doc_failed_other_title
            },
        )
    val body =
        stringResource(
            when (failure) {
                DocumentFailure.NoText -> R.string.doc_failed_no_text_body
                DocumentFailure.Encrypted -> R.string.doc_failed_encrypted_body
                DocumentFailure.TooManyPages -> R.string.doc_failed_pages_body
                DocumentFailure.InvalidPdf -> R.string.doc_failed_invalid_body
                DocumentFailure.Other ->
                    if (retry != null) R.string.doc_failed_other_body else R.string.doc_failed_other_body_no_retry
            },
        )
    var details by rememberSaveable(document.id) { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth().heightIn(min = ROW_MIN_HEIGHT).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm)) {
            TeachIcon(TeachIcons.AlertCircle, contentDescription = null, tint = colors.danger)
            TeachText(
                document.listTitle(),
                Modifier.weight(1f),
                style = TeachTheme.typography.ui.title3,
                color = colors.ink,
            )
        }
        // One panel inside the row, not a second one: icon + title + text, never colour alone (13.5).
        TeachStatusPanel(StatusVariant.Danger, title, body = body, announce = false) {
            val message = document.error?.message?.takeIf { it.isNotBlank() && failure == DocumentFailure.Other }
            if (message != null) {
                TeachDisclosure(stringResource(R.string.doc_details), details, { details = !details }) {
                    SelectionContainer {
                        TeachText(message, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
                    }
                }
            }
        }
        deleteFailed?.let {
            Hint(
                stringResource(
                    if (it.error ==
                        DeleteError.Busy
                    ) {
                        R.string.doc_delete_error_busy
                    } else {
                        R.string.doc_delete_error_network
                    },
                ),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            if (retry != null) {
                TeachButton(
                    stringResource(if (state.retrying) R.string.doc_retrying else R.string.doc_retry),
                    { actions.onRetryImport(document.id) },
                    variant = ButtonVariant.Secondary,
                    loading = state.retrying,
                    leadingIcon = TeachIcons.RotateCcw,
                )
            }
            TeachButton(
                stringResource(if (removing) R.string.doc_removing else R.string.doc_remove),
                { actions.onDelete(document.id) },
                variant = if (retry != null) ButtonVariant.Text else ButtonVariant.Secondary,
                loading = removing,
                contentDescription = stringResource(R.string.doc_remove_a11y, document.listTitle()),
            )
        }
    }
}

/** Invisible polite live region for the end of an import: the learner hears it wherever the focus is. */
@Composable
private fun DocumentAnnouncer(announcement: DocumentAnnouncement?) {
    announcement ?: return
    val text =
        when (announcement) {
            is DocumentAnnouncement.Read ->
                announcement.pages?.let {
                    pluralStringResource(
                        R.plurals.doc_announce_read,
                        it,
                        announcement.title,
                        it,
                    )
                }
                    ?: stringResource(R.string.doc_announce_read_no_pages, announcement.title)
            is DocumentAnnouncement.Failed -> {
                val (title, body) = failureTexts(announcement.failure)
                "$title. $body"
            }
        }
    key(announcement.id) {
        Box(
            Modifier.size(1.dp).semantics {
                liveRegion = LiveRegionMode.Polite
                contentDescription = text
            },
        )
    }
}

@Composable
private fun failureTexts(failure: DocumentFailure): Pair<String, String> {
    val (title, body) =
        when (failure) {
            DocumentFailure.NoText -> R.string.doc_failed_no_text_title to R.string.doc_failed_no_text_body
            DocumentFailure.Encrypted -> R.string.doc_failed_encrypted_title to R.string.doc_failed_encrypted_body
            DocumentFailure.TooManyPages -> R.string.doc_failed_pages_title to R.string.doc_failed_pages_body
            DocumentFailure.InvalidPdf -> R.string.doc_failed_invalid_title to R.string.doc_failed_invalid_body
            DocumentFailure.Other -> R.string.doc_failed_other_title to R.string.doc_failed_other_body
        }
    return stringResource(title) to stringResource(body)
}
