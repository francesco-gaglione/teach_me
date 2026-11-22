package com.teachme.core.model

import kotlinx.coroutines.flow.Flow

enum class FlashKind { FolderGone, FolderDeleted, TopicGone, TopicDeleted, SignedOut, PasswordChanged }

/**
 * A message the next screen shows as a toast after a pop (e.g. "Cartella «Lavoro» eliminata" on Home
 * after deleting from the folder screen).
 */
data class Flash(
    val kind: FlashKind,
    val name: String? = null,
)

/**
 * One-slot mailbox between screens: the screen being closed [post]s a [Flash], the screen revealed by
 * the pop [take]s it in `onStart` and shows the toast.
 */
interface FlashInbox {
    fun post(flash: Flash)

    /** Returns and clears the pending flash if its kind is one of [kinds]; otherwise leaves it. */
    fun take(vararg kinds: FlashKind): Flash?

    /** Drops whatever is pending. */
    fun clear()
}

/**
 * Drops what could carry the data of another user (names of folders and topics) when the session ends or another one
 * starts. The two messages that belong to the sign-in screen, "signed out" and "password changed", carry no data and
 * are posted just before the session ends: they stay.
 */
fun FlashInbox.clearUserFlashes() {
    val keep = take(FlashKind.SignedOut, FlashKind.PasswordChanged)
    clear()
    keep?.let(::post)
}

/**
 * Keeps [inbox] free of other users' messages: it is emptied (see [clearUserFlashes]) whenever the session ends and
 * whenever a session with another token starts, including a forced sign-out (`401`) that no screen announced.
 */
suspend fun Flow<AuthState>.clearFlashesOnSessionChange(inbox: FlashInbox) {
    var token: String? = null
    collect { state ->
        when (state) {
            is AuthState.SignedIn -> {
                if (state.session.token != token) inbox.clearUserFlashes()
                token = state.session.token
            }
            is AuthState.SignedOut -> {
                inbox.clearUserFlashes()
                token = null
            }
            AuthState.Loading -> Unit
        }
    }
}

/** Flashes older than [maxAgeMs] are dropped: a toast must belong to the pop that just happened. */
class InMemoryFlashInbox(
    private val maxAgeMs: Long = FLASH_MAX_AGE_MS,
    private val now: () -> Long = System::currentTimeMillis,
) : FlashInbox {
    private var pending: Flash? = null
    private var postedAt = 0L

    @Synchronized
    override fun post(flash: Flash) {
        pending = flash
        postedAt = now()
    }

    @Synchronized
    override fun clear() {
        pending = null
    }

    @Synchronized
    override fun take(vararg kinds: FlashKind): Flash? {
        val flash = pending ?: return null
        if (now() - postedAt > maxAgeMs) {
            pending = null
            return null
        }
        return flash.takeIf { it.kind in kinds }?.also { pending = null }
    }
}

private const val FLASH_MAX_AGE_MS = 10_000L
