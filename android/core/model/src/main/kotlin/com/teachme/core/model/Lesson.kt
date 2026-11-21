package com.teachme.core.model

/**
 * Where lesson documents come from. In phase 5 a build-specific source serves the contract fixtures; from
 * phase 6 the backend does (`GET /lessons/{id}`). A lesson is addressed by an opaque [key].
 */
interface LessonDocumentSource {
    /** The raw JSON of the lesson, or null when it cannot be loaded. Never throws. */
    suspend fun load(key: String): String?
}

/**
 * Per-lesson progress kept on the device (revealed sections, answers, drafts, scroll position), as an
 * opaque JSON string owned by the lesson renderer. Survives restarts.
 */
interface LessonStateStore {
    suspend fun load(key: String): String?

    /** Fire and forget: the write is queued and survives the caller going away (a screen being closed). */
    fun save(
        key: String,
        json: String,
    )
}
