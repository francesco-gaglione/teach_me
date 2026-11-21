package com.teachme.feature.login

import com.teachme.core.model.SignedOutReason
import com.teachme.core.model.UnreachableReason

/** The four ways the login screen opens (login.md, "Quando si vede"). */
enum class LoginVariant {
    FirstAccess,
    Update,
    Return,
    SessionEnded,
    ;

    /** Intro text and the title are shown only before anyone ever signed in. */
    val showsIntro: Boolean get() = this == FirstAccess || this == Update

    companion object {
        fun of(reason: SignedOutReason): LoginVariant =
            when (reason) {
                SignedOutReason.FirstRun -> FirstAccess
                SignedOutReason.Upgrade -> Update
                SignedOutReason.LoggedOut -> Return
                SignedOutReason.Expired -> SessionEnded
            }
    }
}

/** What the status panel above "Accedi" says; none while the form is just open. */
sealed interface LoginPanel {
    /** `3rd failed attempt in a row` adds the hint about who creates accounts. */
    data class InvalidCredentials(
        val repeated: Boolean,
    ) : LoginPanel

    /** The block is for [username]; [untilMs] is on the monotonic clock. [waitKnown] false: `Retry-After` was missing. */
    data class RateLimited(
        val username: String,
        val untilMs: Long,
        val waitKnown: Boolean,
    ) : LoginPanel

    data class Unreachable(
        val reason: UnreachableReason,
    ) : LoginPanel

    data object NotReady : LoginPanel

    data object NotRecognized : LoginPanel

    data class Unexpected(
        val httpStatus: Int?,
        val requestId: String?,
    ) : LoginPanel
}

/** The unit of the wait shown to the user (login.md, "Troppi tentativi"). */
enum class WaitUnit { Seconds, Minutes, Hours }

data class Wait(
    val unit: WaitUnit,
    val amount: Int,
)

/** Under a minute: seconds. Under an hour: minutes rounded up. Otherwise hours rounded up. */
fun waitOf(seconds: Long): Wait {
    val s = seconds.coerceAtLeast(1)
    return when {
        s < SECONDS_PER_MINUTE -> Wait(WaitUnit.Seconds, s.toInt())
        s < SECONDS_PER_HOUR -> Wait(WaitUnit.Minutes, ((s + SECONDS_PER_MINUTE - 1) / SECONDS_PER_MINUTE).toInt())
        else -> Wait(WaitUnit.Hours, ((s + SECONDS_PER_HOUR - 1) / SECONDS_PER_HOUR).toInt())
    }
}

/** How often the shown wait changes: every second below a minute, every 10 seconds above. */
fun tickMillis(remainingSeconds: Long): Long = if (remainingSeconds < SECONDS_PER_MINUTE) 1_000L else 10_000L

/** Without `Retry-After` the button comes back after a minute. */
internal const val DEFAULT_BLOCK_MS = 60_000L

/** The "takes longer than usual" hint appears after this long. */
internal const val SLOW_HINT_MS = 8_000L

/** From the third failed attempt in a row the panel adds where an account comes from. */
internal const val REPEATED_FAILURES = 3

private const val SECONDS_PER_MINUTE = 60L
private const val SECONDS_PER_HOUR = 3_600L
