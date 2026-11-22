package com.teachme.core.ui

import android.content.res.Resources
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.teachme.core.model.BudgetReset
import com.teachme.core.model.budgetRenewed
import com.teachme.core.model.budgetReset
import com.teachme.core.model.budgetResetAt
import kotlinx.coroutines.delay
import java.time.Clock
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/** Title of every panel that says today's LLM budget is used up (llm_budget_exceeded). */
@Composable
fun llmBudgetTitle(): String = LocalResources.current.getString(R.string.ui_budget_title)

/**
 * "Hai usato tutto il budget giornaliero del modello. Si rinnova domani alle 01:00": the reset is 00:00 UTC, written in
 * the device's zone. [retryAfterSecs] is the server's Retry-After when the failure came with one; job and message errors
 * have none.
 */
@Composable
fun llmBudgetBody(retryAfterSecs: Int? = null): String = LocalResources.current.llmBudgetBody(retryAfterSecs)

/** The same body for a reset instant that was fixed when the failure was seen (see [com.teachme.core.model.budgetResetInstant]). */
@Composable
fun llmBudgetBody(resetAt: Instant): String = LocalResources.current.llmBudgetBody(resetAt)

fun Resources.llmBudgetBody(
    retryAfterSecs: Int? = null,
    clock: Clock = Clock.systemDefaultZone(),
    zone: ZoneId = ZoneId.systemDefault(),
): String = budgetBody(budgetReset(clock.instant(), zone, retryAfterSecs))

fun Resources.llmBudgetBody(
    resetAt: Instant,
    clock: Clock = Clock.systemDefaultZone(),
    zone: ZoneId = ZoneId.systemDefault(),
): String = budgetBody(budgetResetAt(resetAt, clock.instant(), zone))

private fun Resources.budgetBody(reset: BudgetReset): String =
    getString(if (reset.tomorrow) R.string.ui_budget_body_tomorrow else R.string.ui_budget_body_today, localTime(reset))

private fun Resources.localTime(reset: BudgetReset): String {
    val locale = configuration.locales.get(0) ?: Locale.getDefault()
    return DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale).format(reset.at)
}

/** "Disponibile oggi alle 01:00": the state description of a "Riprova" or "Invia" that waits for the budget. */
fun Resources.llmBudgetAvailability(
    resetAt: Instant,
    clock: Clock = Clock.systemDefaultZone(),
    zone: ZoneId = ZoneId.systemDefault(),
): String {
    val reset = budgetResetAt(resetAt, clock.instant(), zone)
    val day = getString(if (reset.tomorrow) R.string.ui_budget_day_tomorrow else R.string.ui_budget_day_today)
    return getString(R.string.ui_budget_retry_available, day, localTime(reset))
}

@Composable
fun llmBudgetAvailability(resetAt: Instant): String = LocalResources.current.llmBudgetAvailability(resetAt)

/**
 * Whether the budget wait is over (llm-key.md section 10.1): false until the device clock reaches [resetAt], then true.
 * One timer runs until that instant and it is recomputed when the app comes back to the foreground, so a sleeping
 * device or a changed clock does not leave the button disabled. When the wait ends while this is on screen it is
 * announced once, politely, without moving the focus ([announce] false when another element on screen already does).
 * There is no polling of the server.
 */
@Composable
fun rememberBudgetRenewed(
    resetAt: Instant?,
    clock: Clock = Clock.systemUTC(),
    announce: Boolean = true,
): Boolean {
    val view = LocalView.current
    val message = LocalResources.current.getString(R.string.ui_budget_renewed)
    var renewed by remember(resetAt) { mutableStateOf(budgetRenewed(resetAt, clock.instant())) }
    // Only a wait that was seen ending is announced: a panel that opens already renewed says nothing.
    var waited by remember(resetAt) { mutableStateOf(!renewed) }

    fun renew() {
        if (renewed) return
        renewed = true
        if (waited && announce) view.announceForAccessibility(message)
    }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
        if (budgetRenewed(resetAt, clock.instant())) renew()
    }
    LaunchedEffect(resetAt, renewed) {
        if (resetAt == null || renewed) return@LaunchedEffect
        while (!budgetRenewed(resetAt, clock.instant())) {
            delay(
                java.time.Duration
                    .between(clock.instant(), resetAt)
                    .toMillis()
                    .coerceAtLeast(1L),
            )
        }
        renew()
    }
    return renewed
}
