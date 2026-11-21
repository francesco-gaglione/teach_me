package com.teachme.core.model

/** Keeps the cached [AuthUser] fresh with `GET /me` (users.md): at start, back in the foreground, Settings opening. */
interface SessionRefresher {
    /** Asks the backend now. A network failure keeps the cache; a `401` is handled by the global handler. */
    suspend fun refresh()

    /** Same, but at most once per minute (foreground returns). */
    suspend fun refreshIfStale()
}

/** Leaving the session on purpose. */
interface SessionActions {
    /**
     * Ends the local session at once (token, nothing else) and asks the backend to drop it in the background, best
     * effort with a short timeout: whatever the answer, the user is out.
     */
    suspend fun signOut()
}

/** What is still on this device waiting to be sent for the current user. */
data class PendingCounts(
    val reviews: Int = 0,
    val answers: Int = 0,
    /** Lessons whose highlights have not reached the backend yet. */
    val highlights: Int = 0,
    /** Lessons whose handwritten notes have not reached the backend yet. */
    val strokes: Int = 0,
) {
    val total: Int get() = reviews + answers + highlights + strokes
    val isEmpty: Boolean get() = total == 0
}

/** The persistent queues (review grades, lesson answers and highlights) seen as one, for sign-out and the login notice. */
interface PendingSync {
    suspend fun counts(): PendingCounts

    /** Tries to send everything for at most [timeoutMs] and returns what is left. */
    suspend fun flush(timeoutMs: Long): PendingCounts
}
