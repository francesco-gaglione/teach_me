package com.teachme.feature.settings

import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

class SuspendedLogicTest {
    private val rome = ZoneId.of("Europe/Rome")
    private val now = Instant.parse("2026-11-16T10:00:00Z")

    private fun at(text: String) = suspendedWhen(Instant.parse(text), now, rome)

    @Test
    fun `today and yesterday are named`() {
        assertEquals(SuspendedWhen.Today, at("2026-11-16T08:00:00Z"))
        assertEquals(SuspendedWhen.Yesterday, at("2026-11-15T20:00:00Z"))
    }

    @Test
    fun `the day is the local one, not the UTC one`() {
        // 23:30 UTC on the 15th is already the 16th in Rome (UTC+1): today.
        assertEquals(SuspendedWhen.Today, at("2026-11-15T23:30:00Z"))
    }

    @Test
    fun `older days carry the date and the year only when it is not the current one`() {
        assertEquals(SuspendedWhen.On(LocalDate.of(2026, 11, 12), withYear = false), at("2026-11-12T09:00:00Z"))
        assertEquals(SuspendedWhen.On(LocalDate.of(2025, 11, 12), withYear = true), at("2025-11-12T09:00:00Z"))
    }

    @Test
    fun `a time in the future reads as today`() {
        assertEquals(SuspendedWhen.Today, at("2026-11-20T09:00:00Z"))
    }

    @Test
    fun `the button name uses the first 60 characters of the front`() {
        assertEquals(60, "x".repeat(100).frontForButton().length)
        assertEquals("breve", " breve ".frontForButton())
    }
}
