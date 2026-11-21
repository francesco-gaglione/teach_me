package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.LocalDate
import java.util.Locale

class LocaleDatesTest {
    private val day = LocalDate.of(2026, 11, 8)

    @Test
    fun `day and month follow the locale`() {
        assertEquals("8 novembre", day.format(dayMonthFormatter(Locale.ITALIAN)))
        assertEquals("November 8", day.format(dayMonthFormatter(Locale.US)))
    }

    @Test
    fun `the year is added only on request`() {
        assertEquals("8 novembre 2026", day.format(dayMonthFormatter(Locale.ITALIAN, withYear = true)))
        assertEquals("November 8, 2026", day.format(dayMonthFormatter(Locale.US, withYear = true)))
    }

    @Test
    fun `abbreviated month`() {
        assertEquals("8 nov", day.format(dayMonthFormatter(Locale.ITALIAN, abbreviated = true)))
        assertEquals("Nov 8", day.format(dayMonthFormatter(Locale.US, abbreviated = true)))
    }

    @Test
    fun `languages without translated strings use Italian`() {
        assertEquals("8 novembre", day.format(dayMonthFormatter(Locale.JAPAN)))
        assertEquals("8 novembre 2026", day.format(dayMonthFormatter(Locale("hu"), withYear = true)))
        assertEquals(Locale.ITALIAN, contentLocale(Locale.FRANCE))
        assertEquals(Locale.US, contentLocale(Locale.US))
    }
}
