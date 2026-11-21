package com.teachme.core.designsystem.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme

/** Share of the limit from which the character counter is shown (base-components 10.1). */
private const val COUNTER_THRESHOLD = 0.8f

/** Number of Unicode code points of the trimmed text: what the backend counts. */
fun String.trimmedLength(): Int = trim().let { it.codePointCount(0, it.length) }

/**
 * Input dialog (base-components 10.1): one short text, two actions.
 *
 * The typed text lives here in `rememberSaveable` state (synchronous, survives rotation) and never in
 * a ViewModel flow. Validation: `empty` only on submit, `too_long` live; both produce no network call.
 * [serverFieldError] (from a 422) shows under the field and disappears on the first edit, which is
 * reported through [onTextEdited]. [error] is the network/server panel above the actions.
 * Scrim taps do not close the dialog (the text would be lost by accident); Back and Cancel do.
 *
 * @param initialText prefilled text, fully selected.
 * @param tooLongText text for the live `too_long` error, given the current length.
 */
@Composable
fun TeachInputDialog(
    title: String,
    label: String,
    confirmLabel: String,
    loadingLabel: String,
    dismissLabel: String,
    maxLength: Int,
    emptyText: String,
    tooLongText: (currentLength: Int) -> String,
    onConfirm: (text: String) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    initialText: String = "",
    loading: Boolean = false,
    serverFieldError: String? = null,
    error: DialogErrorPanel? = null,
    onTextEdited: () -> Unit = {},
    /** Extra controls shown under the text field (for example a choice that goes with the name). */
    extraContent: (@Composable ColumnScope.() -> Unit)? = null,
) {
    var text by rememberSaveable(stateSaver = TextFieldValue.Saver) {
        mutableStateOf(TextFieldValue(initialText, TextRange(0, initialText.length)))
    }
    var showEmpty by rememberSaveable { mutableStateOf(false) }
    val length = text.text.trimmedLength()
    val tooLong = length > maxLength
    val showCounter = length >= maxLength * COUNTER_THRESHOLD
    val fieldError =
        when {
            tooLong -> tooLongText(length)
            showEmpty && length == 0 -> emptyText
            else -> serverFieldError
        }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    val stacked = LocalDensity.current.fontScale > LARGE_FONT_SCALE

    val submit = {
        if (!loading) {
            if (length == 0) {
                showEmpty = true
            } else if (!tooLong) {
                onConfirm(text.text.trim())
            }
        }
    }

    TeachDialogShell(title, onDismiss, blocked = loading, dismissOnScrimTap = false) {
        TeachText(title, style = TeachTheme.typography.ui.title2)
        TeachTextField(
            value = text,
            onValueChange = { new ->
                // A single-paragraph field: pasted line breaks become spaces.
                val clean =
                    if (new.text.contains('\n') || new.text.contains('\r')) {
                        new.copy(
                            text =
                                new.text
                                    .replace("\r\n", " ")
                                    .replace('\n', ' ')
                                    .replace('\r', ' '),
                        )
                    } else {
                        new
                    }
                val changed = clean.text != text.text
                text = clean
                if (changed) {
                    showEmpty = false
                    onTextEdited()
                }
            },
            label = label,
            modifier = modifier.fillMaxWidth(),
            fieldModifier = Modifier.focusRequester(focus),
            placeholder = placeholder,
            errorText = fieldError,
            counter = if (showCounter) "$length/$maxLength" else null,
            counterIsError = tooLong,
            readOnly = loading,
            singleLine = false,
            maxLines = 3,
            keyboardOptions =
                KeyboardOptions(
                    capitalization = KeyboardCapitalization.Sentences,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Done,
                ),
            keyboardActions = KeyboardActions(onDone = { submit() }),
        )
        extraContent?.invoke(this)
        if (error != null) DialogErrorContent(error)
        val confirmText = if (loading) loadingLabel else confirmLabel
        if (stacked) {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                TeachButton(confirmText, submit, Modifier.fillMaxWidth(), loading = loading)
                TeachButton(
                    dismissLabel,
                    onDismiss,
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Text,
                    enabled = !loading,
                )
            }
        } else {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs, Alignment.End),
            ) {
                TeachButton(dismissLabel, onDismiss, variant = ButtonVariant.Text, enabled = !loading)
                TeachButton(confirmText, submit, loading = loading)
            }
        }
    }
}

@Preview(name = "InputDialog light", showBackground = true)
@Composable
private fun InputDialogLightPreview() =
    TeachTheme(darkTheme = false) {
        TeachInputDialog(
            "Nuova cartella",
            "Nome della cartella",
            "Crea",
            "Creo…",
            "Annulla",
            80,
            "Dai un nome alla cartella.",
            { "Al massimo 80 caratteri: ora sono $it." },
            {},
            {},
            placeholder = "Es. Lavoro",
        )
    }

@Preview(name = "InputDialog dark, duplicate", showBackground = true)
@Composable
private fun InputDialogDuplicatePreview() =
    TeachTheme(darkTheme = true) {
        TeachInputDialog(
            "Rinomina cartella",
            "Nome della cartella",
            "Salva",
            "Salvo…",
            "Annulla",
            80,
            "Dai un nome alla cartella.",
            { "Al massimo 80 caratteri: ora sono $it." },
            {},
            {},
            initialText = "Lavoro",
            serverFieldError = "Hai già una cartella con questo nome. Scegline un altro.",
        )
    }

@Preview(name = "InputDialog network error", showBackground = true)
@Composable
private fun InputDialogErrorPreview() =
    TeachTheme(darkTheme = false) {
        TeachInputDialog(
            "Nuova cartella",
            "Nome della cartella",
            "Crea",
            "Creo…",
            "Annulla",
            80,
            "Dai un nome alla cartella.",
            { "Al massimo 80 caratteri: ora sono $it." },
            {},
            {},
            initialText = "Lavoro",
            error =
                DialogErrorPanel(
                    "Non riesco a raggiungere il backend",
                    "La cartella non è stata creata. Riprova quando sei di nuovo collegato.",
                ),
        )
    }

@Preview(name = "InputDialog loading font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun InputDialogLoadingPreview() =
    TeachTheme(darkTheme = false) {
        TeachInputDialog(
            "Nuova cartella",
            "Nome della cartella",
            "Crea",
            "Creo…",
            "Annulla",
            80,
            "Dai un nome alla cartella.",
            { "Al massimo 80 caratteri: ora sono $it." },
            {},
            {},
            initialText = "Lavoro",
            loading = true,
        )
    }
