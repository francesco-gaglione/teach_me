package com.teachme.core.model

import java.time.DayOfWeek
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import kotlin.math.roundToLong

/** Unit of a spoken interval: the screen turns it into an Android plural ("tra 3 giorni"). */
enum class IntervalUnit { Minute, Hour, Day, Month, Year }

/** "tra [count] [unit]" before it is translated (review.md section 4.5). */
data class IntervalPhrase(
    val unit: IntervalUnit,
    val count: Int,
)

private const val MINUTE = 60.0
private const val HOUR = 3_600.0
private const val DAY = 86_400.0
private const val DAYS_PER_MONTH = 30.4375
private const val DAYS_PER_YEAR = 365.25
private const val MINUTES_PER_HOUR = 60
private const val HOURS_PER_DAY = 24
private const val DAYS_IN_30 = 30
private const val DAYS_IN_365 = 365
private const val MONTHS_PER_YEAR = 12

/** Rounds half up, like the spec's "arrotondamento al più vicino". */
private fun Double.roundHalfUp(): Long = kotlin.math.floor(this + 0.5).roundToLong()

/**
 * An interval in seconds as words (review.md section 4.5). Rounded to the nearest unit; when the rounding reaches the
 * next unit the phrase moves to it (59.6 minutes is "1 ora"). Null for `seconds <= 0`: nothing is written.
 */
fun intervalPhrase(seconds: Long): IntervalPhrase? {
    if (seconds <= 0) return null
    val s = seconds.toDouble()
    if (s < MINUTE) return IntervalPhrase(IntervalUnit.Minute, 1)
    if (s < HOUR) {
        val m = (s / MINUTE).roundHalfUp().toInt()
        return if (m >=
            MINUTES_PER_HOUR
        ) {
            IntervalPhrase(IntervalUnit.Hour, 1)
        } else {
            IntervalPhrase(IntervalUnit.Minute, m)
        }
    }
    if (s < DAY) {
        val h = (s / HOUR).roundHalfUp().toInt()
        return if (h >= HOURS_PER_DAY) IntervalPhrase(IntervalUnit.Day, 1) else IntervalPhrase(IntervalUnit.Hour, h)
    }
    val d = s / DAY
    if (d < DAYS_IN_30) {
        val n = d.roundHalfUp().toInt()
        if (n < DAYS_IN_30) return IntervalPhrase(IntervalUnit.Day, n)
        // 29.5 days and more round to 30: the month rule takes over.
    }
    if (d < DAYS_IN_365) {
        val mo = maxOf(1L, (d / DAYS_PER_MONTH).roundHalfUp()).toInt()
        if (mo < MONTHS_PER_YEAR) return IntervalPhrase(IntervalUnit.Month, mo)
    }
    return IntervalPhrase(IntervalUnit.Year, maxOf(1L, (d / DAYS_PER_YEAR).roundHalfUp()).toInt())
}

/** "La prossima scheda arriva {x}" before it is translated (review.md section 4.5). */
sealed interface NextDuePhrase {
    /** Less than an hour away: the interval rule ("tra 10 minuti"). */
    data class In(
        val interval: IntervalPhrase,
    ) : NextDuePhrase

    /** Later the same local day: "oggi alle 18:30". */
    data class TodayAt(
        val time: LocalTime,
    ) : NextDuePhrase

    data object Tomorrow : NextDuePhrase

    /** From 2 to 6 days away: the name of the day. */
    data class OnWeekday(
        val day: DayOfWeek,
    ) : NextDuePhrase

    /** A week or more away: "il 24 novembre". */
    data class OnDate(
        val date: LocalDate,
    ) : NextDuePhrase
}

private const val WEEK_DAYS = 7L

/** [nextDue] relative to [now] in the zone [zone] (the app's clock: a few minutes of error do not matter). */
fun nextDuePhrase(
    nextDue: Instant,
    now: Instant,
    zone: ZoneId,
): NextDuePhrase {
    val delta = Duration.between(now, nextDue)
    if (delta.seconds < HOUR.toLong()) {
        // Already due or about to be: "tra 1 minuto" is the closest honest thing.
        return NextDuePhrase.In(
            intervalPhrase(delta.seconds.coerceAtLeast(1)) ?: IntervalPhrase(IntervalUnit.Minute, 1),
        )
    }
    val dueDay = nextDue.atZone(zone).toLocalDate()
    val today = now.atZone(zone).toLocalDate()
    val days =
        java.time.temporal.ChronoUnit.DAYS
            .between(today, dueDay)
    return when {
        days <= 0L ->
            NextDuePhrase.TodayAt(
                nextDue
                    .atZone(zone)
                    .toLocalTime()
                    .withSecond(0)
                    .withNano(0),
            )
        days == 1L -> NextDuePhrase.Tomorrow
        days < WEEK_DAYS -> NextDuePhrase.OnWeekday(dueDay.dayOfWeek)
        else -> NextDuePhrase.OnDate(dueDay)
    }
}
