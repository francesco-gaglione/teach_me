package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.R
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

/**
 * Synchronous, saveable state of a text field. The text lives in Compose, never in an asynchronously
 * collected flow, so the IME always talks to the latest value. Changing [resetKey] replaces the text
 * with [initial] (e.g. stored values finished loading, a form was normalized): that is the one and only
 * way for the caller to overwrite what the user typed.
 */
@Composable
fun rememberTextFieldValue(
    initial: String = "",
    resetKey: Any? = null,
): MutableState<TextFieldValue> =
    rememberSaveable(resetKey, stateSaver = TextFieldValue.Saver) {
        mutableStateOf(TextFieldValue(initial, TextRange(initial.length)))
    }

/**
 * Text field (base-components section 2): label always visible above, 48 dp bordered input, optional
 * trailing slot, then an error (icon + text, never color alone) or a hint. The state is owned by the
 * caller as a [TextFieldValue], see [rememberTextFieldValue]. Masked fields: pass
 * `PasswordVisualTransformation()` and `KeyboardOptions` with keyboardType Password and
 * autoCorrectEnabled = false. [onFocusLost] fires only after the field had focus. [counter] is shown at the end of the hint/error row;
 * [counterIsError] paints it with the danger color. [readOnly] keeps the look but blocks edits.
 * [fieldModifier] applies to the input itself (e.g. a `focusRequester`).
 */
@Composable
fun TeachTextField(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    fieldModifier: Modifier = Modifier,
    placeholder: String? = null,
    supportingText: String? = null,
    errorText: String? = null,
    counter: String? = null,
    counterIsError: Boolean = false,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    singleLine: Boolean = true,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    trailing: (@Composable () -> Unit)? = null,
    onFocusLost: (() -> Unit)? = null,
) {
    val colors = TeachTheme.colors
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    var hadFocus by remember { mutableStateOf(false) }
    val hasError = errorText != null

    val borderColor =
        when {
            !enabled -> colors.disabledFill
            hasError -> colors.danger
            focused -> colors.focus
            else -> colors.border
        }
    val borderWidth = if (focused || hasError) TeachTheme.stroke.selected else TeachTheme.stroke.control
    val fill = if (enabled) colors.surface else colors.disabledFill

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
        TeachText(label, style = TeachTheme.typography.ui.label, color = colors.inkSoft)
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = TeachTheme.layout.controlHeight)
                    .background(fill, TeachTheme.shapes.sm)
                    .border(borderWidth, borderColor, TeachTheme.shapes.sm)
                    .padding(start = TeachTheme.spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicTextField(
                value = value,
                onValueChange = onValueChange,
                enabled = enabled,
                readOnly = readOnly,
                modifier =
                    fieldModifier
                        .weight(1f)
                        .padding(vertical = TeachTheme.spacing.sm)
                        .onFocusChanged {
                            if (it.isFocused) {
                                hadFocus = true
                            } else if (hadFocus) {
                                hadFocus = false
                                onFocusLost?.invoke()
                            }
                        }.semantics {
                            if (errorText != null) error(errorText)
                        },
                textStyle =
                    TeachTheme.typography.ui.bodyL.copy(
                        color = if (enabled) colors.ink else colors.disabledInk,
                    ),
                cursorBrush = SolidColor(colors.accent),
                singleLine = singleLine,
                maxLines = maxLines,
                keyboardOptions = keyboardOptions,
                keyboardActions = keyboardActions,
                visualTransformation = visualTransformation,
                interactionSource = source,
                decorationBox = { inner ->
                    Box(contentAlignment = Alignment.CenterStart) {
                        if (value.text.isEmpty() && placeholder != null) {
                            TeachText(placeholder, style = TeachTheme.typography.ui.bodyL, color = colors.inkMuted)
                        }
                        inner()
                    }
                },
            )
            trailing?.invoke()
        }
        if (errorText != null || supportingText != null || counter != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                Box(Modifier.weight(1f)) {
                    if (errorText != null) {
                        Row(horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                            TeachIcon(
                                TeachIcons.AlertCircle,
                                stringResource(R.string.teach_state_error_icon),
                                tint = colors.danger,
                                size = TeachTheme.layout.iconSizeSmall,
                            )
                            TeachText(errorText, style = TeachTheme.typography.ui.bodyS, color = colors.danger)
                        }
                    } else if (supportingText != null) {
                        TeachText(supportingText, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
                    }
                }
                if (counter != null) {
                    TeachText(
                        counter,
                        style = TeachTheme.typography.ui.bodyS,
                        color = if (counterIsError) colors.danger else colors.inkSoft,
                    )
                }
            }
        }
    }
}

@Composable
private fun FieldsSample() {
    Column(
        modifier =
            Modifier
                .background(TeachTheme.colors.bg)
                .padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        val url = rememberTextFieldValue("https://teach.local")
        TeachTextField(url.value, { url.value = it }, "URL del backend", supportingText = "Es. https://teach.local")
        val token = rememberTextFieldValue("0123456789")
        TeachTextField(
            value = token.value,
            onValueChange = { token.value = it },
            label = "Token",
            visualTransformation = PasswordVisualTransformation(),
            errorText = "Il token sembra troppo corto.",
            trailing = { TeachIconButton(TeachIcons.Eye, "Mostra token", {}) },
        )
        val empty = rememberTextFieldValue()
        TeachTextField(empty.value, { empty.value = it }, "Vuoto", placeholder = "Segnaposto")
        TeachTextField(TextFieldValue("Non modificabile"), {}, "Disabilitato", enabled = false)
    }
}

@Preview(name = "Fields light", showBackground = true)
@Composable
private fun FieldsLightPreview() = TeachTheme(darkTheme = false) { FieldsSample() }

@Preview(name = "Fields dark", showBackground = true)
@Composable
private fun FieldsDarkPreview() = TeachTheme(darkTheme = true) { FieldsSample() }

@Preview(name = "Fields font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun FieldsLargeFontPreview() = TeachTheme(darkTheme = false) { FieldsSample() }
