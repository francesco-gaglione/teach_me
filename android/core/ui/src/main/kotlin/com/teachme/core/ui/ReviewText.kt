package com.teachme.core.ui

import android.content.res.Resources
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalResources
import com.teachme.core.model.IntervalPhrase
import com.teachme.core.model.IntervalUnit
import com.teachme.core.model.NextDuePhrase
import com.teachme.core.model.dayMonthFormatter
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

private val HOUR_MINUTE: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")

private fun Resources.locale(): Locale = configuration.locales.get(0) ?: Locale.getDefault()

/** "tra 10 minuti", with Android plurals (review.md section 4.5). */
fun Resources.intervalText(phrase: IntervalPhrase): String {
    val plural =
        when (phrase.unit) {
            IntervalUnit.Minute -> R.plurals.ui_interval_minutes
            IntervalUnit.Hour -> R.plurals.ui_interval_hours
            IntervalUnit.Day -> R.plurals.ui_interval_days
            IntervalUnit.Month -> R.plurals.ui_interval_months
            IntervalUnit.Year -> R.plurals.ui_interval_years
        }
    return getQuantityString(plural, phrase.count, phrase.count)
}

/** The interval of [seconds] as words, or null when there is nothing to say (the screen then writes nothing). */
fun Resources.intervalText(seconds: Long?): String? =
    seconds
        ?.let {
            com.teachme.core.model
                .intervalPhrase(it)
        }?.let { intervalText(it) }

/** "La prossima scheda arriva domani." and its siblings. */
fun Resources.nextDueSentence(phrase: NextDuePhrase): String =
    when (phrase) {
        is NextDuePhrase.In -> getString(R.string.ui_next_due_on, intervalText(phrase.interval))
        is NextDuePhrase.TodayAt -> getString(R.string.ui_next_due_today, HOUR_MINUTE.format(phrase.time))
        NextDuePhrase.Tomorrow -> getString(R.string.ui_next_due_tomorrow)
        is NextDuePhrase.OnWeekday ->
            getString(R.string.ui_next_due_on, phrase.day.getDisplayName(TextStyle.FULL, locale()))
        is NextDuePhrase.OnDate ->
            getString(
                R.string.ui_next_due_on,
                getString(
                    R.string.ui_next_due_date,
                    phrase.date.format(dayMonthFormatter(locale())),
                ),
            )
    }

@Composable
fun intervalTextOrNull(seconds: Long?): String? = LocalResources.current.intervalText(seconds)

@Composable
fun nextDueSentenceText(phrase: NextDuePhrase): String = LocalResources.current.nextDueSentence(phrase)
