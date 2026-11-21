package com.teachme.core.model

/** A validation problem the backend attributes to the form field (folders-topics.md section 3). */
enum class FieldIssue { Empty, TooLong, Duplicate }

/** Why an action (create, rename, delete) failed when it is not a field problem. */
enum class ActionFailure { Network, Unauthorized, Unexpected }

/** Failure of a dialog action: either a field issue shown under the input or an action-level failure. */
data class FormError(
    val field: FieldIssue? = null,
    val action: ActionFailure? = null,
)

/**
 * Maps a failed call to what a dialog shows. [fieldName] is the JSON field of the input (`name` or
 * `title`); a 422 about any other field, or an unknown issue, is [ActionFailure.Unexpected]. Callers
 * handle [ApiFailure.NotFound] before calling this (the target is gone: that is not a form error).
 */
fun ApiFailure.toFormError(fieldName: String): FormError =
    when (this) {
        is ApiFailure.Network -> FormError(action = ActionFailure.Network)
        ApiFailure.Unauthorized -> FormError(action = ActionFailure.Unauthorized)
        is ApiFailure.Validation -> {
            val issue =
                details
                    .firstOrNull { it.field == fieldName }
                    ?.issue
                    ?.let {
                        when (it) {
                            "empty" -> FieldIssue.Empty
                            "too_long" -> FieldIssue.TooLong
                            "duplicate" -> FieldIssue.Duplicate
                            else -> null
                        }
                    }
            if (issue != null) FormError(field = issue) else FormError(action = ActionFailure.Unexpected)
        }
        else -> FormError(action = ActionFailure.Unexpected)
    }

/** Why a screen could not load its data; drives the full-screen error. */
enum class LoadError { Network, Unauthorized, Server }

fun ApiFailure.toLoadError(): LoadError =
    when (this) {
        is ApiFailure.Network -> LoadError.Network
        ApiFailure.Unauthorized -> LoadError.Unauthorized
        else -> LoadError.Server
    }
