package com.teachme.core.model

import kotlinx.coroutines.withContext

/** What the review notification must do after a check (ADR 0012, point 4). */
sealed interface ReviewAlertDecision {
    /** No cards are due: take the notification away. */
    data object Clear : ReviewAlertDecision

    /** Show or update the single notification. [alert] is true only when it should make noise (the count grew). */
    data class Show(
        val dueCount: Int,
        val estimatedMinutes: Int,
        val alert: Boolean,
    ) : ReviewAlertDecision
}

/** The rule of ADR 0012 point 4, pure: it alerts only when `due_count` grows over the last notified value. */
object ReviewAlertRule {
    /** [reference] is the last notified count of this user; the new reference is [newReference] of the result. */
    fun decide(
        dueCount: Int,
        estimatedMinutes: Int,
        reference: Int,
    ): ReviewAlertDecision =
        if (dueCount <= 0) {
            ReviewAlertDecision.Clear
        } else {
            ReviewAlertDecision.Show(dueCount, estimatedMinutes.coerceAtLeast(0), alert = dueCount > reference)
        }

    /** The count to remember after [decision]: 0 when cleared, otherwise the count just shown (also when it dropped). */
    fun newReference(decision: ReviewAlertDecision): Int =
        when (decision) {
            ReviewAlertDecision.Clear -> 0
            is ReviewAlertDecision.Show -> decision.dueCount
        }
}

/** The last count that was notified, one value per user (ADR 0012 point 4). Implemented in `:core:data`. */
interface ReviewAlertReference {
    suspend fun get(scope: UserScope): Int

    suspend fun set(
        scope: UserScope,
        count: Int,
    )
}

/** The one review notification, with a fixed id. Implemented in `:app`. */
interface ReviewNotifier {
    /**
     * Posts or updates it in place. With [alert] false it updates silently, and only if the notification is still on
     * screen: one the user dismissed or tapped is not brought back. Returns false only when nothing could be posted
     * (notifications blocked), so the caller keeps the old reference and the growth is not used up.
     */
    fun show(
        dueCount: Int,
        estimatedMinutes: Int,
        alert: Boolean,
    ): Boolean

    fun cancel()
}

/** What a check did, for the worker (and the tests). */
enum class ReviewCheckOutcome {
    /** The notification was shown, updated or cleared. */
    Done,

    /** Nobody is signed in: nothing asked, notification cleared. */
    NoSession,

    /** The session is no longer valid (`401`): notification cleared, reference reset. */
    SessionEnded,

    /** Network or server error: nothing changed, the next hourly run tries again. */
    Failed,
}

/** One check of the due cards: `GET /reviews/due?limit=1`, then the rule, the notification and the stored reference. */
class ReviewReminderCheck(
    private val sessions: SessionRepository,
    private val reviews: ReviewRepository,
    private val reference: ReviewAlertReference,
    private val notifier: ReviewNotifier,
) {
    suspend fun run(): ReviewCheckOutcome {
        // A cold worker starts while the stored session is still being read: `Loading` is not "nobody signed in".
        val scope = (sessions.awaitReady() as? AuthState.SignedIn)?.session?.scope
        if (scope == null) {
            notifier.cancel()
            return ReviewCheckOutcome.NoSession
        }
        // The HTTP client refuses to send this with the token of another user.
        val result = withContext(ExpectedUserScope(scope)) { reviews.due(1) }
        return when (result) {
            is ApiResult.Failure ->
                if (result.failure is ApiFailure.Unauthorized) {
                    notifier.cancel()
                    reference.set(scope, 0)
                    ReviewCheckOutcome.SessionEnded
                } else {
                    ReviewCheckOutcome.Failed
                }

            is ApiResult.Success -> {
                // Another user signed in while the request was out: this answer is not theirs.
                val still = (sessions.state.value as? AuthState.SignedIn)?.session?.scope
                if (still != scope) return ReviewCheckOutcome.Failed
                val decision =
                    ReviewAlertRule.decide(result.value.dueCount, result.value.estimatedMinutes, reference.get(scope))
                val handled =
                    when (decision) {
                        ReviewAlertDecision.Clear -> {
                            notifier.cancel()
                            true
                        }
                        is ReviewAlertDecision.Show ->
                            notifier.show(decision.dueCount, decision.estimatedMinutes, decision.alert)
                    }
                // A blocked notification must not use up the growth: the reference stays until one is really posted.
                if (handled) reference.set(scope, ReviewAlertRule.newReference(decision))
                ReviewCheckOutcome.Done
            }
        }
    }
}
