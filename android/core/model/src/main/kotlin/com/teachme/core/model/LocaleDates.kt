package com.teachme.core.model

import java.time.chrono.IsoChronology
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeFormatterBuilder
import java.time.format.FormatStyle
import java.util.Locale

private val YEAR_PART = Regex("[\\s,./-]*y+")

/**
 * The locale dates use: the same as the UI strings, which exist in Italian (the default) and English only. So the
 * device language when it is Italian or English, otherwise Italian.
 */
fun contentLocale(device: Locale = Locale.getDefault()): Locale =
    if (device.language == "it" || device.language == "en") device else Locale.ITALIAN

/**
 * A day-and-month formatter that follows [locale] ("8 novembre", "November 8"), with the year when [withYear].
 * [abbreviated] gives the short month name ("8 nov"). Never a numeric format.
 */
fun dayMonthFormatter(
    device: Locale = Locale.getDefault(),
    withYear: Boolean = false,
    abbreviated: Boolean = false,
): DateTimeFormatter {
    val locale = contentLocale(device)
    val style = if (abbreviated) FormatStyle.MEDIUM else FormatStyle.LONG
    val pattern = DateTimeFormatterBuilder.getLocalizedDateTimePattern(style, null, IsoChronology.INSTANCE, locale)
    return DateTimeFormatter.ofPattern(if (withYear) pattern else pattern.replace(YEAR_PART, "").trim(), locale)
}
