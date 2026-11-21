package com.teachme.sdui.logic

import com.teachme.sdui.model.Exercise

/** Lookup of what the learner did, by block id. */
typealias ExerciseLookup = (String) -> ExerciseState

/** Unlocking and progress rules of docs/design/screens/lesson.md section 5. Pure. */
object LessonProgress {
    /** Required exercises of [section] that still wait for an answer, in reading order. */
    fun missing(
        section: Section,
        states: ExerciseLookup,
    ): List<Exercise> = section.requiredExercises.filter { !states(it.id).done }

    /** "Continua" is unlocked when no required exercise is missing. A wrong answer counts. */
    fun isUnlocked(
        section: Section,
        states: ExerciseLookup,
    ): Boolean = missing(section, states).isEmpty()

    /**
     * Progress bar value 0..1: `(k - 1 + e/E) / N` with `k` the number of the current section, `E` its
     * required exercises, `e` those answered (0 when E = 0). 1 once the lesson is completed.
     */
    fun value(
        sections: LessonSections,
        current: Section,
        states: ExerciseLookup,
        completed: Boolean,
    ): Float {
        if (completed) return 1f
        val n = sections.numberedCount
        if (n == 0) return 0f
        val k = current.number ?: 0
        val required = current.requiredExercises
        val fraction = if (required.isEmpty()) 0f else required.count { states(it.id).done }.toFloat() / required.size
        return ((k - 1 + fraction) / n).coerceIn(0f, 1f)
    }

    /** True for the sections the learner already passed with "Continua": their exercises are read-only. */
    fun isLocked(
        sectionIndex: Int,
        revealedCount: Int,
    ): Boolean = sectionIndex < revealedCount - 1
}
