package com.teachme.core.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue

/**
 * The text of a password or key field. It lives in the ViewModel as Compose state: synchronous for the IME (no characters
 * lost to an asynchronously collected flow), kept across rotation, and never written anywhere: not in `SavedStateHandle`,
 * not in `rememberSaveable`, not in a log. A killed process starts with empty fields. [toString] hides the value.
 */
class SecretField {
    var value: TextFieldValue by mutableStateOf(TextFieldValue())
        private set

    /** The characters, for the one place that sends them. */
    val text: String get() = value.text

    fun onChange(newValue: TextFieldValue) {
        value = newValue
    }

    fun set(text: String) {
        value = TextFieldValue(text, TextRange(text.length))
    }

    /** Empties the field and forgets the text. */
    fun clear() {
        value = TextFieldValue()
    }

    /** Selects the whole text, so that typing replaces it (login.md, wrong credentials). */
    fun selectAll() {
        value = value.copy(selection = TextRange(0, value.text.length))
    }

    override fun toString(): String = "SecretField(length hidden)"
}
