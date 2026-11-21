package com.teachme.core.ui

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import com.teachme.core.designsystem.components.DialogErrorPanel
import com.teachme.core.designsystem.components.TeachConfirmDialog
import com.teachme.core.designsystem.components.TeachInputDialog
import com.teachme.core.model.ActionFailure
import com.teachme.core.model.FieldIssue
import com.teachme.core.model.FormError

/** What a form dialog shows besides the typed text (which the dialog keeps itself). */
data class DialogUi(
    val submitting: Boolean = false,
    val error: FormError? = null,
)

/** Texts of one create/rename dialog. [notDone] is the sentence "La cartella non è stata creata." */
data class NameDialogTexts(
    val title: String,
    val label: String,
    val placeholder: String?,
    val confirm: String,
    val loading: String,
    val empty: String,
    val duplicate: String,
    val notDone: String,
)

@Composable
fun actionPanel(
    failure: ActionFailure,
    notDone: String,
    @Suppress("UNUSED_PARAMETER") onOpenSettings: () -> Unit,
): DialogErrorPanel? =
    when (failure) {
        ActionFailure.Network ->
            DialogErrorPanel(
                stringResource(R.string.ui_error_network_title),
                "$notDone ${stringResource(R.string.ui_error_network_retry_hint)}",
            )
        // A 401 is the global handler's business (login.md): the dialog shows nothing and keeps the typed text.
        ActionFailure.Unauthorized -> null
        ActionFailure.Unexpected ->
            DialogErrorPanel(
                stringResource(R.string.ui_error_unexpected_title),
                "$notDone ${stringResource(R.string.ui_error_unexpected_retry_hint)}",
            )
    }

/** Create/rename dialog for a folder name or a topic title, wired to a [DialogUi]. */
@Composable
fun NameInputDialog(
    texts: NameDialogTexts,
    maxLength: Int,
    ui: DialogUi,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
    onTextEdited: () -> Unit,
    onOpenSettings: () -> Unit,
    initialText: String = "",
    extraContent: (@Composable ColumnScope.() -> Unit)? = null,
) {
    val serverError =
        when (ui.error?.field) {
            FieldIssue.Empty -> texts.empty
            FieldIssue.TooLong -> stringResource(R.string.ui_too_long_server, maxLength)
            FieldIssue.Duplicate -> texts.duplicate
            null -> null
        }
    val panel = ui.error?.action?.let { actionPanel(it, texts.notDone, onOpenSettings) }
    val resources = LocalResources.current
    TeachInputDialog(
        title = texts.title,
        label = texts.label,
        confirmLabel = texts.confirm,
        loadingLabel = texts.loading,
        dismissLabel = stringResource(R.string.ui_cancel),
        maxLength = maxLength,
        emptyText = texts.empty,
        tooLongText = { count -> resources.getString(R.string.ui_too_long_live, maxLength, count) },
        onConfirm = onConfirm,
        onDismiss = onDismiss,
        placeholder = texts.placeholder,
        initialText = initialText,
        loading = ui.submitting,
        serverFieldError = serverError,
        error = panel,
        onTextEdited = onTextEdited,
        extraContent = extraContent,
    )
}

/** Destructive confirmation wired to a [DialogUi]; [notDone] is "La cartella non è stata eliminata." */
@Composable
fun DeleteConfirmDialog(
    title: String,
    body: String,
    confirmLabel: String,
    loadingLabel: String,
    notDone: String,
    ui: DialogUi,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    onOpenSettings: () -> Unit,
) {
    val failure = ui.error?.action
    TeachConfirmDialog(
        title = title,
        body = body,
        confirmLabel = confirmLabel,
        dismissLabel = stringResource(R.string.ui_cancel),
        onConfirm = onConfirm,
        onDismiss = onDismiss,
        destructive = true,
        loading = ui.submitting,
        loadingLabel = loadingLabel,
        error = failure?.let { actionPanel(it, notDone, onOpenSettings) },
    )
}
