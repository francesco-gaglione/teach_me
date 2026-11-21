package com.teachme.feature.settings

import com.teachme.core.model.ApiErrorDetail

internal const val PASSWORD_MIN = 10
internal const val PASSWORD_MAX = 128

/** Length in Unicode code points, the way the backend counts (settings.md, "Cambia password"). */
internal fun passwordLength(text: String): Int = text.codePointCount(0, text.length)

/** A problem with a field of the change-password form; [length] is the typed length for the "Ora sono n" hint. */
sealed interface PasswordFieldError {
    data object CurrentEmpty : PasswordFieldError

    data object CurrentWrong : PasswordFieldError

    data object NewEmpty : PasswordFieldError

    data class TooShort(
        val length: Int?,
    ) : PasswordFieldError

    data object TooLong : PasswordFieldError

    data object Mismatch : PasswordFieldError
}

/** The local check of the new password; null when it is acceptable. */
internal fun validateNewPassword(text: String): PasswordFieldError? {
    val length = passwordLength(text)
    return when {
        text.isEmpty() -> PasswordFieldError.NewEmpty
        length < PASSWORD_MIN -> PasswordFieldError.TooShort(length)
        length > PASSWORD_MAX -> PasswordFieldError.TooLong
        else -> null
    }
}

/** Maps the `422` details of `PUT /me/password` to field errors (users.md section 2.3). */
internal fun serverPasswordErrors(details: List<ApiErrorDetail>): Pair<PasswordFieldError?, PasswordFieldError?> {
    var current: PasswordFieldError? = null
    var new: PasswordFieldError? = null
    for (d in details) {
        when (d.field) {
            "current_password" -> if (d.issue == "wrong") current = PasswordFieldError.CurrentWrong
            "new_password" ->
                new =
                    when (d.issue) {
                        "too_short" -> PasswordFieldError.TooShort(null)
                        "too_long" -> PasswordFieldError.TooLong
                        else -> new
                    }
        }
    }
    return current to new
}
