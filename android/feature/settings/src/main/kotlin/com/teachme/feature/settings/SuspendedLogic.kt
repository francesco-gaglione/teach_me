package com.teachme.feature.settings

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/** When a card was suspended, relative to today in the local zone (suspended.md section 2). */
sealed interface SuspendedWhen {
    data object Today : SuspendedWhen

    data object Yesterday : SuspendedWhen

    /** [withYear] only when the year differs from the current one. */
    data class On(
        val date: LocalDate,
        val withYear: Boolean,
    ) : SuspendedWhen
}

fun suspendedWhen(
    at: Instant,
    now: Instant,
    zone: ZoneId,
): SuspendedWhen {
    val day = at.atZone(zone).toLocalDate()
    val today = now.atZone(zone).toLocalDate()
    return when {
        // A clock that is a little behind the server never shows a date in the future.
        !day.isBefore(today) -> SuspendedWhen.Today
        day == today.minusDays(1) -> SuspendedWhen.Yesterday
        else -> SuspendedWhen.On(day, withYear = day.year != today.year)
    }
}

/** First characters of the front, enough to tell the button apart when a screen reader lists buttons. */
internal const val FRONT_DESCRIPTION_CHARS = 60

internal fun String.frontForButton(): String = trim().take(FRONT_DESCRIPTION_CHARS)
