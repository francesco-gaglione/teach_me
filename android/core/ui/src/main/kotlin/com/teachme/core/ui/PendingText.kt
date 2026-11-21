package com.teachme.core.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import com.teachme.core.model.PendingCounts

/**
 * "3 voti di ripasso", "2 risposte di lezioni", "le evidenziazioni di 1 lezione" or the ones that apply joined with "e" (login.md): the number
 * is in the text because here it helps to decide. Empty when nothing is waiting.
 */
@Composable
fun pendingPhrase(counts: PendingCounts): String {
    val parts =
        listOfNotNull(
            counts.reviews.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.ui_pending_reviews, it, it) },
            counts.answers.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.ui_pending_answers, it, it) },
            counts.highlights.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.ui_pending_highlights, it, it) },
            counts.strokes.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.ui_pending_strokes, it, it) },
        )
    return when (parts.size) {
        0 -> ""
        1 -> parts.first()
        else -> stringResource(R.string.ui_pending_both, parts.dropLast(1).joinToString(", "), parts.last())
    }
}
