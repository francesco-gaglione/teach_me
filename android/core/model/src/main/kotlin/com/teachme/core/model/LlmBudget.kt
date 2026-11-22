package com.teachme.core.model

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.ZonedDateTime

/** The `error.code` of a job or a chat message that stopped because the daily LLM budget is used up. */
const val LLM_BUDGET_EXCEEDED_CODE = "llm_budget_exceeded"

/** True for [LLM_BUDGET_EXCEEDED_CODE]; the screens that read the code of a failed job or message use it. */
fun String?.isLlmBudgetCode(): Boolean = this == LLM_BUDGET_EXCEEDED_CODE

/** When the daily budget comes back: [at] in the user's zone, and whether that is tomorrow (local date) rather than today. */
data class BudgetReset(
    val at: ZonedDateTime,
    val tomorrow: Boolean,
)

/**
 * The instant the budget comes back for a failure seen at [anchor]: the next 00:00 UTC after it (the budget is per UTC
 * day, api-conventions section 12). With a [retryAfterSecs] from the server it is `anchor + retryAfterSecs` instead: the
 * same moment, as the server measured it. It is fixed once computed: callers keep it, they do not ask again.
 */
fun budgetResetInstant(
    anchor: Instant,
    retryAfterSecs: Int? = null,
): Instant =
    if (retryAfterSecs != null && retryAfterSecs > 0) {
        anchor.plusSeconds(retryAfterSecs.toLong())
    } else {
        val utcDay: LocalDate = anchor.atZone(ZoneOffset.UTC).toLocalDate()
        utcDay.plusDays(1).atStartOfDay(ZoneOffset.UTC).toInstant()
    }

/**
 * When the budget comes back if this failure is `llm_budget_exceeded` (its Retry-After, else the next 00:00 UTC), seen at
 * [nowMs]; null for any other failure. Screens keep the result: it must not move while the panel is on screen.
 */
fun ApiFailure.budgetResetAt(nowMs: Long): Instant? =
    (this as? ApiFailure.LlmBudgetExceeded)?.let { budgetResetInstant(Instant.ofEpochMilli(nowMs), it.retryAfterSecs) }

/**
 * When the budget comes back for a failed job: counted from its own failure time (a failed job carries no Retry-After),
 * or [refused] when a later "Riprova" was turned down again and that answer says it is later.
 */
fun TopicJob.jobBudgetReset(refused: Instant? = null): Instant {
    val own = budgetResetInstant(finishedAt ?: createdAt)
    return if (refused != null && refused.isAfter(own)) refused else own
}

/** True once [now] has reached [resetAt]; a null [resetAt] (no budget wait) counts as reached. */
fun budgetRenewed(
    resetAt: Instant?,
    now: Instant,
): Boolean = resetAt == null || !now.isBefore(resetAt)

/** [resetAt] written for the user: in [zone], and whether it is on a later local day than [now]. */
fun budgetResetAt(
    resetAt: Instant,
    now: Instant,
    zone: ZoneId,
): BudgetReset {
    val at = resetAt.atZone(zone)
    return BudgetReset(at, tomorrow = at.toLocalDate().isAfter(now.atZone(zone).toLocalDate()))
}

/** [budgetResetInstant] for [now], written for the user in [zone]. */
fun budgetReset(
    now: Instant,
    zone: ZoneId,
    retryAfterSecs: Int? = null,
): BudgetReset = budgetResetAt(budgetResetInstant(now, retryAfterSecs), now, zone)
