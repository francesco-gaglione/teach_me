package com.teachme.sdui.logic

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * Everything persisted for one lesson (docs/design/screens/lesson.md section 2): revealed sections,
 * per-block exercise state, opened glossary terms, panel disclosures and the scroll position. Restored on
 * reopening and after a restart. State of ids that no longer exist in the document is dropped by [prunedTo].
 */
@Serializable
data class LessonSavedState(
    /** How many sections are visible (at least 1). */
    val revealedCount: Int = 1,
    val exercises: Map<String, ExerciseState> = emptyMap(),
    val openedTerms: List<String> = emptyList(),
    /** Context panel disclosures: section key (`index`, `glossary`, `sources`) -> open. */
    val disclosures: Map<String, Boolean> = emptyMap(),
    val scrollBlockId: String? = null,
    val scrollOffsetPx: Int = 0,
    val completed: Boolean = false,
    /** "Sezione k di N" at the last write, so the lessons list can say "In corso · 2 di 4 sezioni" (0 = unknown). */
    val sectionNumber: Int = 0,
    val sectionTotal: Int = 0,
    /** Fingerprint of the document this state belongs to: a changed document starts from scratch. */
    val docHash: String = "",
) {
    fun exercise(id: String): ExerciseState = exercises[id] ?: ExerciseState()

    fun withExercise(
        id: String,
        state: ExerciseState,
    ): LessonSavedState = copy(exercises = exercises + (id to state))

    /** Drops state of blocks the (regenerated) document no longer has and clamps the revealed count. */
    fun prunedTo(
        blockIds: Set<String>,
        sectionCount: Int,
    ): LessonSavedState =
        copy(
            revealedCount = revealedCount.coerceIn(1, maxOf(1, sectionCount)),
            exercises = exercises.filterKeys { it in blockIds },
            // The scroll anchor is a row key: block rows are "b:<id>", other rows (title extras, bars) stay valid.
            scrollBlockId =
                scrollBlockId?.takeIf {
                    !it.startsWith(BLOCK_KEY_PREFIX) ||
                        it.removePrefix(BLOCK_KEY_PREFIX) in blockIds
                },
        )

    fun encode(): String = JSON.encodeToString(serializer(), this)

    companion object {
        /** Prefix of the list row key of a block (see `LessonItem.BlockItem`). */
        const val BLOCK_KEY_PREFIX = "b:"

        /** Short stable fingerprint of a document text. */
        fun hashOf(text: String): String =
            java.security.MessageDigest
                .getInstance("SHA-256")
                .digest(text.toByteArray(Charsets.UTF_8))
                .take(HASH_BYTES)
                .joinToString("") { "%02x".format(it) }

        private const val HASH_BYTES = 12

        private val JSON =
            Json {
                ignoreUnknownKeys = true
                encodeDefaults = false
            }

        /** Never throws: damaged or unreadable data gives a fresh state. */
        fun decode(text: String?): LessonSavedState =
            if (text.isNullOrBlank()) {
                LessonSavedState()
            } else {
                try {
                    JSON.decodeFromString(serializer(), text)
                } catch (e: kotlinx.serialization.SerializationException) {
                    LessonSavedState()
                } catch (e: IllegalArgumentException) {
                    LessonSavedState()
                }
            }
    }
}
