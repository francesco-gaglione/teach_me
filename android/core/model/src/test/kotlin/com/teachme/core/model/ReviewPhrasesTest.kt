package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId

class ReviewPhrasesTest {
    private fun phrase(seconds: Long) = intervalPhrase(seconds)?.let { it.unit to it.count }

    private val day = 86_400L

    @Test
    fun `under a minute is one minute, never seconds`() {
        assertEquals(IntervalUnit.Minute to 1, phrase(1))
        assertEquals(IntervalUnit.Minute to 1, phrase(20))
        assertEquals(IntervalUnit.Minute to 1, phrase(59))
    }

    @Test
    fun `minutes round to the nearest and 60 seconds is one minute`() {
        assertEquals(IntervalUnit.Minute to 1, phrase(60))
        assertEquals(IntervalUnit.Minute to 2, phrase(90))
        assertEquals(IntervalUnit.Minute to 10, phrase(600))
        assertEquals(IntervalUnit.Minute to 59, phrase(3_540))
    }

    @Test
    fun `minutes that round up to an hour become one hour (3599 and 3600)`() {
        assertEquals(IntervalUnit.Hour to 1, phrase(3_599))
        assertEquals(IntervalUnit.Hour to 1, phrase(3_570))
        assertEquals(IntervalUnit.Hour to 1, phrase(3_600))
        assertEquals(IntervalUnit.Minute to 59, phrase(3_569))
    }

    @Test
    fun `hours round and 23 and a half hours or more is one day (86399 and 86400)`() {
        assertEquals(IntervalUnit.Hour to 5, phrase(18_000))
        assertEquals(IntervalUnit.Hour to 2, phrase(5_400))
        assertEquals(IntervalUnit.Hour to 23, phrase(23 * 3_600L))
        assertEquals(IntervalUnit.Day to 1, phrase(86_399))
        assertEquals(IntervalUnit.Day to 1, phrase(day))
    }

    @Test
    fun `days round to the nearest, half a day goes up`() {
        assertEquals(IntervalUnit.Day to 2, phrase(172_800))
        assertEquals(IntervalUnit.Day to 11, phrase(950_400))
        assertEquals(IntervalUnit.Day to 2, phrase(129_600))
        assertEquals(IntervalUnit.Day to 1, phrase(day + 43_199))
    }

    @Test
    fun `29 and a half days is already a month, 29 days is not`() {
        assertEquals(IntervalUnit.Day to 29, phrase(29 * day))
        assertEquals(IntervalUnit.Day to 29, phrase(29 * day + day / 2 - 1))
        assertEquals(IntervalUnit.Month to 1, phrase(29 * day + day / 2))
        assertEquals(IntervalUnit.Month to 1, phrase(31 * day))
    }

    @Test
    fun `months use 30,4375 days and 364 days is one year`() {
        assertEquals(IntervalUnit.Month to 2, phrase(60 * day))
        assertEquals(IntervalUnit.Month to 6, phrase(183 * day))
        assertEquals(IntervalUnit.Month to 11, phrase(335 * day))
        assertEquals(IntervalUnit.Year to 1, phrase(364 * day))
    }

    @Test
    fun `years`() {
        assertEquals(IntervalUnit.Year to 1, phrase(365 * day))
        assertEquals(IntervalUnit.Year to 1, phrase(400 * day))
        assertEquals(IntervalUnit.Year to 3, phrase(1_100 * day))
    }

    @Test
    fun `zero, negative and missing intervals write nothing`() {
        assertNull(intervalPhrase(0))
        assertNull(intervalPhrase(-5))
        assertNull(IntervalsPreview(null, 0, -1, 5).forRating(ReviewRating.Again))
        assertNull(IntervalsPreview(null, 0, -1, 5).forRating(ReviewRating.Hard))
        assertNull(IntervalsPreview(null, 0, -1, 5).forRating(ReviewRating.Good))
        assertEquals(5L, IntervalsPreview(null, 0, -1, 5).forRating(ReviewRating.Easy))
    }

    private val zone = ZoneId.of("Europe/Rome")

    // 2026-11-10 (a Tuesday), 10:00 local.
    private val now = Instant.parse("2026-11-10T09:00:00Z")

    private fun next(iso: String) = nextDuePhrase(Instant.parse(iso), now, zone)

    @Test
    fun `less than an hour away is an interval`() {
        assertEquals(NextDuePhrase.In(IntervalPhrase(IntervalUnit.Minute, 10)), next("2026-11-10T09:10:00Z"))
        assertEquals(NextDuePhrase.In(IntervalPhrase(IntervalUnit.Hour, 1)), next("2026-11-10T09:59:59Z"))
        // Already due by the app's clock: the closest honest thing.
        assertEquals(NextDuePhrase.In(IntervalPhrase(IntervalUnit.Minute, 1)), next("2026-11-10T08:59:00Z"))
    }

    @Test
    fun `the same local day is today at a time, 24 hours`() {
        assertEquals(NextDuePhrase.TodayAt(LocalTime.of(18, 30)), next("2026-11-10T17:30:00Z"))
        assertEquals(NextDuePhrase.TodayAt(LocalTime.of(11, 0)), next("2026-11-10T10:00:00Z"))
    }

    @Test
    fun `the local calendar decides, not 24 hour blocks`() {
        // 23:30 local on the 10th is today; 00:30 local on the 11th is tomorrow although only 1 hour later.
        assertEquals(NextDuePhrase.TodayAt(LocalTime.of(23, 30)), next("2026-11-10T22:30:00Z"))
        assertEquals(NextDuePhrase.Tomorrow, next("2026-11-10T23:30:00Z"))
        assertEquals(NextDuePhrase.Tomorrow, next("2026-11-11T17:00:00Z"))
    }

    @Test
    fun `two to six days is the weekday, a week or more is the date`() {
        assertEquals(NextDuePhrase.OnWeekday(DayOfWeek.THURSDAY), next("2026-11-12T09:00:00Z"))
        assertEquals(NextDuePhrase.OnWeekday(DayOfWeek.MONDAY), next("2026-11-16T09:00:00Z"))
        assertEquals(NextDuePhrase.OnDate(LocalDate.of(2026, 11, 17)), next("2026-11-17T09:00:00Z"))
        assertEquals(NextDuePhrase.OnDate(LocalDate.of(2026, 11, 24)), next("2026-11-24T09:00:00Z"))
    }

    @Test
    fun `session minutes count one session at most`() {
        assertEquals(0, sessionMinutes(0))
        assertEquals(1, sessionMinutes(1))
        assertEquals(1, sessionMinutes(4))
        assertEquals(2, sessionMinutes(5))
        assertEquals(3, sessionMinutes(12))
        assertEquals(5, sessionMinutes(20))
        assertEquals(5, sessionMinutes(87))
    }
}
