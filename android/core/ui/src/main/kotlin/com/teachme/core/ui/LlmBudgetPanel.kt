package com.teachme.core.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachStatusPanel
import java.time.Instant

/**
 * "Budget di oggi esaurito" (llm-key.md section 10.1): the one panel for every place where an LLM action was refused or
 * failed because the daily budget is used up. Warning variant with the clock icon, a polite live region, no dismiss.
 * Not a key notice: there is nothing to do but wait.
 *
 * [resetAt] is the reset instant fixed when the failure was seen. The retry button ([onRetry], labelled [retryText]) is
 * visible but disabled until the device clock reaches it, with the state description "Disponibile oggi alle HH:MM"; it
 * enables itself (one timer, recomputed on return to the foreground). [bodySuffix] is the reassurance that follows the
 * body ("Il tuo messaggio è ancora nel campo."). Without [onRetry] only the text is shown.
 */
@Composable
fun LlmBudgetPanel(
    resetAt: Instant,
    modifier: Modifier = Modifier,
    bodySuffix: String? = null,
    retryText: String? = null,
    onRetry: (() -> Unit)? = null,
    retrying: Boolean = false,
    retryIcon: Int? = null,
    /** False when another budget panel on the same screen already announces the renewal (one voice per screen). */
    announce: Boolean = true,
) {
    // The panel's own live region says the change; the helper's separate announcement stays off so it is said once.
    val renewed = rememberBudgetRenewed(resetAt, announce = false)
    // Already past (an old failed job or message, or the timer just fired): no "si rinnova alle 01:00" in the past.
    val body =
        (if (renewed) stringResource(R.string.ui_budget_renewed) else llmBudgetBody(resetAt))
            .let { if (bodySuffix.isNullOrBlank()) it else "$it $bodySuffix" }
    val availability = llmBudgetAvailability(resetAt)
    TeachStatusPanel(
        if (renewed) StatusVariant.Info else StatusVariant.Warning,
        if (renewed) stringResource(R.string.ui_budget_renewed_title) else llmBudgetTitle(),
        modifier,
        body = body,
        icon = TeachIcons.Clock,
        announce = announce,
    ) {
        if (onRetry != null && retryText != null) {
            TeachButton(
                text = retryText,
                onClick = onRetry,
                variant = ButtonVariant.Secondary,
                enabled = renewed,
                loading = retrying,
                leadingIcon = retryIcon,
                disabledStateDescription = availability,
            )
        }
    }
}

@Composable
private fun BudgetSamples() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        val future = Instant.now().plusSeconds(3 * 3600L)
        // Waiting: the button is visible but disabled.
        LlmBudgetPanel(future, retryText = "Riprova", onRetry = {}, retryIcon = TeachIcons.RotateCcw)
        // Renewed: the button is enabled.
        LlmBudgetPanel(
            Instant.now().minusSeconds(60),
            retryText = "Riprova",
            onRetry = {},
            retryIcon = TeachIcons.RotateCcw,
        )
        // Without an action (an older failed message).
        LlmBudgetPanel(future, bodySuffix = "Il tuo messaggio è ancora nel campo.")
    }
}

@Preview(name = "Budget panel light", showBackground = true)
@Composable
private fun BudgetLightPreview() = TeachTheme(darkTheme = false) { BudgetSamples() }

@Preview(name = "Budget panel dark", showBackground = true)
@Composable
private fun BudgetDarkPreview() = TeachTheme(darkTheme = true) { BudgetSamples() }
