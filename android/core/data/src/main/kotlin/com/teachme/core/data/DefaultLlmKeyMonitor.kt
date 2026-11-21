package com.teachme.core.data

import android.content.Context
import android.util.Log
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.AppClock
import com.teachme.core.model.AuthState
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.SessionRepository
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.Instant
import javax.inject.Inject
import javax.inject.Singleton

private val Context.llmKeyDataStore: DataStore<Preferences> by preferencesDataStore(name = "llm_key")

/** Legacy: scopes with a remembered refusal and no instant (read as refused at the moment of the migration). */
private val KEY_REJECTED = stringSetPreferencesKey("rejected_scopes")

/** `"<epoch millis>:<scope key>"` entries: when the refusal of each scope happened, when each key was last known good... */
private val KEY_REJECTED_AT = stringSetPreferencesKey("rejected_at")
private val KEY_GOOD_AT = stringSetPreferencesKey("key_good_at")

/** ...and the latest key failure ever seen for each scope, even one that was ignored as stale. */
private val KEY_SEEN_AT = stringSetPreferencesKey("key_failure_seen_at")

private fun Set<String>.toInstants(): Map<String, Long> =
    mapNotNull { entry ->
        val at = entry.substringBefore(':').toLongOrNull()
        if (at == null) null else entry.substringAfter(':') to at
    }.groupBy({ it.first }, { it.second }).mapValues { it.value.max() }

private fun Map<String, Long>.raised(
    key: String,
    at: Long,
): Map<String, Long> = if ((this[key] ?: Long.MIN_VALUE) >= at) this else this + (key to at)

/**
 * The known state of the signed-in user's LLM key (llm-key.md section 2): `has_llm_key` from the session, plus what was
 * seen of its health, kept per user and backend so it survives a restart and never leaks to another user.
 *
 * A failure is judged by its date, not by the channel it came from: per user the monitor keeps the instant of the last
 * proof that the key is fine (`goodAt`: a key saved here, an LLM operation that worked) and ignores any observation of a
 * failure that is not later. So the same failure can be reported again from any place without re-arming a state that was
 * resolved. Failure instants come from the server, and `goodAt` is dated by the server too, never compared with the device
 * clock: from the instant the success carries (a job, a message) or, when it carries none (a key saved here, a grade),
 * from the `Date` header of the response that proved it. That header has a precision of one second while failure
 * instants have milliseconds, so the proof is dated at the LAST millisecond of its second: a failure inside that second
 * counts as older (the safe side: a replayed old failure must not re-arm what the user just fixed; a really new failure
 * in the very same second is missed until the next one). A failure born from the answer of a request is dated the same way,
 * by the START of the second of that answer's `Date` (proofs by its end, so in a shared second the failure is the older one).
 * A `Date` more than a day ahead of the later of the device clock and `seenAt` is a mistake and is ignored. Only a response with no usable `Date` falls back to the device's
 * clock, never earlier than the latest key failure already seen (`seenAt`), so a clock that runs behind cannot let an old
 * failure come back; in that degenerate case a clock that runs ahead can hide a new failure until the next one.
 * `goodAt`, `seenAt` and the refusal instant are persisted; the credit is memory only (llm-key.md 9.4), is tied to the
 * session (its token, so a sign-out and a new sign-in, even of the same user, start without it however fast they follow
 * each other) and counts only if it failed after this monitor started for that session. The version counter is in memory:
 * it only compares "before" and "after".
 *
 * Nothing is accepted before the first read of the disk is over: calls made meanwhile wait and run in order after it.
 */
@Singleton
class DefaultLlmKeyMonitor(
    private val sessions: SessionRepository,
    private val store: DataStore<Preferences>,
    private val appScope: CoroutineScope,
    private val clock: AppClock = AppClock { System.currentTimeMillis() },
) : LlmKeyMonitor {
    @Inject
    constructor(
        sessions: SessionRepository,
        @ApplicationContext context: Context,
    ) : this(sessions, context.llmKeyDataStore, CoroutineScope(SupervisorJob() + Dispatchers.IO))

    // Scope key -> epoch millis of the refusal / of the last proof the key is fine / of the latest key failure seen.
    private val rejected = MutableStateFlow<Map<String, Long>>(emptyMap())
    private val goodAt = MutableStateFlow<Map<String, Long>>(emptyMap())
    private val seenAt = MutableStateFlow<Map<String, Long>>(emptyMap())
    private val version = MutableStateFlow(0)

    // In memory only (llm-key.md 9.4): a credit that ran out is nothing to remember across runs. Both maps are keyed by
    // the token of the session, not by the user: a sign-out followed by a sign-in (even of the same user) is another
    // session, so nothing of the old one counts, whatever the timing of the collector below.
    private val creditAt = MutableStateFlow<Map<String, Long>>(emptyMap())

    // When this process started handling each session: an older credit failure is not news after a restart or a sign-in.
    private val startedAt = MutableStateFlow<Map<String, Long>>(emptyMap())
    private val processStart = clock.nowMs()

    private val lock = Any()
    private var loaded = false
    private val waiting = mutableListOf<() -> Unit>()

    override val status: StateFlow<LlmKeyStatus> =
        combine(sessions.state, rejected, version, creditAt) { auth, refusals, v, credit ->
            when (auth) {
                is AuthState.SignedIn -> {
                    val user = auth.session.user
                    val state =
                        when {
                            !user.hasLlmKey -> LlmKeyState.Missing
                            auth.session.scope.key in refusals -> LlmKeyState.RejectedKnown
                            else -> LlmKeyState.Set
                        }
                    LlmKeyStatus(state, v, user.llmProvider, auth.session.token in credit)
                }
                else -> LlmKeyStatus(LlmKeyState.Set, v)
            }
        }.stateIn(appScope, SharingStarted.Eagerly, LlmKeyStatus())

    init {
        // Per-user memory goes with the session: whoever signs in next (or the same user again) starts clean.
        (sessions.state.value as? AuthState.SignedIn)?.let { markStarted(it.session.token) }
        appScope.launch {
            // Housekeeping only: correctness does not depend on this collector (the maps are keyed by session).
            sessions.state.collect {
                if (it is AuthState.SignedIn) {
                    markStarted(it.session.token)
                } else {
                    creditAt.value = emptyMap()
                    startedAt.value = emptyMap()
                }
            }
        }
        appScope.launch {
            val prefs =
                try {
                    store.data.first()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    // Unreadable disk (IO or anything else): start from what is known in memory, never stay mute.
                    Log.w(TAG, "cannot read the key health from the disk (${e::class.simpleName})")
                    null
                }
            synchronized(lock) {
                try {
                    if (prefs != null) restore(prefs)
                } finally {
                    // The gate opens and the queue drains whatever happened: one failing operation must not strand the rest.
                    loaded = true
                    val queued = waiting.toList()
                    waiting.clear()
                    queued.forEach { op ->
                        try {
                            op()
                        } catch (_: Exception) {
                            // Dropped: the others still run.
                        }
                    }
                }
            }
        }
    }

    /** Idempotent and synchronous: the first sight of a session (here or in the collector) is its start. */
    private fun markStarted(key: String): Long {
        startedAt.update { if (key in it) it else it + (key to clock.nowMs()) }
        return startedAt.value.getValue(key)
    }

    /**
     * The `Date` of a response as long as it is believable: not more than a day ahead of the later of the device clock
     * and the latest key failure seen for [key]. A proxy or a wrong server saying 2099 would otherwise push the proof
     * ahead for ever (it is persisted) and hide every real failure; such a date is ignored as if it were missing.
     */
    private fun trusted(
        serverDate: Instant?,
        key: String,
    ): Instant? {
        if (serverDate == null) return null
        val ceiling = maxOf(clock.nowMs(), seenAt.value[key] ?: Long.MIN_VALUE) + MAX_DATE_AHEAD_MS
        return serverDate.takeIf { it.toEpochMilli() <= ceiling }
    }

    /** Runs [op] now, or right after the first read of the disk when it is not over yet (in the order of the calls). */
    private fun afterLoad(op: () -> Unit) =
        synchronized(lock) {
            if (loaded) op() else waiting += op
        }

    private fun restore(prefs: Preferences) {
        val good = prefs[KEY_GOOD_AT]?.toInstants().orEmpty()
        val legacy = prefs[KEY_REJECTED].orEmpty()
        // A refusal stored without an instant is dated now: only a proof later than this run can clear it.
        val stored = legacy.associateWith { clock.nowMs() } + prefs[KEY_REJECTED_AT]?.toInstants().orEmpty()
        goodAt.value = good
        seenAt.value = prefs[KEY_SEEN_AT]?.toInstants().orEmpty()
        rejected.value = stored.filter { (k, at) -> at > (good[k] ?: Long.MIN_VALUE) }
        legacy.forEach(::persist)
    }

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: Instant,
    ) {
        // Bound to whoever is signed in now: a late call must not land on the user who signs in after.
        val auth = sessions.state.value as? AuthState.SignedIn ?: return
        afterLoad { observe(auth, variant, at.toEpochMilli(), fresh = false) }
    }

    override fun noteFailedNow(
        variant: LlmKeyVariant,
        serverDate: Instant?,
    ) {
        val auth = sessions.state.value as? AuthState.SignedIn ?: return
        val nowMs = clock.nowMs()
        afterLoad {
            // Start of the second of the server's Date (see the interface); the device's clock only without a usable one.
            val atMs =
                trusted(
                    serverDate,
                    auth.session.scope.key,
                )?.toEpochMilli()?.let { it - Math.floorMod(it, 1000L) }
            observe(auth, variant, atMs ?: nowMs, fresh = true)
        }
    }

    private fun observe(
        auth: AuthState.SignedIn,
        variant: LlmKeyVariant,
        atMs: Long,
        fresh: Boolean,
    ) {
        val key = auth.session.scope.key
        val token = auth.session.token
        val started = markStarted(token)
        // Seen even when it is stale: whatever the app saw before a key is saved here must never come back. Only a refusal
        // or a missing key counts: the credit is not a key state, and must not push a later save ahead of the device clock.
        var changed = false
        if (variant != LlmKeyVariant.Credit && atMs > (seenAt.value[key] ?: Long.MIN_VALUE)) {
            seenAt.update { it.raised(key, atMs) }
            changed = true
        }
        // A failure not later than the last proof that the key is fine is old news, whoever tells it.
        if (atMs > (goodAt.value[key] ?: Long.MIN_VALUE)) {
            when (variant) {
                // The cache said there was a key and the backend says there is none: believe the backend.
                LlmKeyVariant.Missing ->
                    if (auth.session.user.hasLlmKey) {
                        appScope.launch { sessions.updateUser(auth.session.user.copy(hasLlmKey = false)) }
                    }
                LlmKeyVariant.Rejected ->
                    if (atMs > (rejected.value[key] ?: Long.MIN_VALUE)) {
                        rejected.update { it.raised(key, atMs) }
                        changed = true
                    }
                // The service accepted the key, so an older refusal (from before the backend told them apart) is stale.
                LlmKeyVariant.Credit -> {
                    // A failure that just came back from a request is new by construction. One that is read from a job or a
                    // message must be later than the start of the session: server instant against the device clock, so a
                    // skewed device may drop or admit one near the start (accepted, nothing else tells the two apart).
                    if (fresh || atMs >= started) creditAt.update { it.raised(token, atMs) }
                    if ((rejected.value[key] ?: Long.MAX_VALUE) <= atMs) {
                        rejected.update { it - key }
                        changed = true
                    }
                }
            }
        }
        if (changed) persist(key)
    }

    override fun noteWorked(at: Instant) {
        val auth = sessions.state.value as? AuthState.SignedIn ?: return
        val atMs = at.toEpochMilli()
        afterLoad { proveGood(auth, atMs) }
    }

    override fun noteWorkedNow() {
        val auth = sessions.state.value as? AuthState.SignedIn ?: return
        val nowMs = clock.nowMs()
        val key = auth.session.scope.key
        afterLoad { proveGood(auth, maxOf(nowMs, seenAt.value[key] ?: Long.MIN_VALUE)) }
    }

    override fun noteWorkedAtServerDate(serverDate: Instant?) {
        val auth = sessions.state.value as? AuthState.SignedIn ?: return
        val nowMs = clock.nowMs()
        afterLoad {
            // Checked after the load: the ceiling depends on what was seen on the disk.
            val date = trusted(serverDate, auth.session.scope.key)
            if (date == null) {
                proveGood(auth, maxOf(nowMs, seenAt.value[auth.session.scope.key] ?: Long.MIN_VALUE))
            } else {
                proveGood(auth, date.endOfSecondMs())
            }
        }
    }

    override fun noteSaved(serverDate: Instant?) {
        val auth = sessions.state.value as? AuthState.SignedIn
        if (auth != null) {
            val key = auth.session.scope.key
            // With the server's Date, that is the proof. Without it, the device's clock, never before a failure already
            // seen: nothing else tells the server's time for a key saved here.
            val nowMs = clock.nowMs()
            afterLoad {
                // Whatever refusal is remembered was about the old key, even if the server's clock put it a little ahead.
                if (key in rejected.value) rejected.update { it - key }
                val provenAt =
                    trusted(serverDate, key)?.endOfSecondMs() ?: maxOf(nowMs, seenAt.value[key] ?: Long.MIN_VALUE)
                proveGood(auth, provenAt)
                persist(key)
            }
        }
        version.update { it + 1 }
    }

    override fun noteRemoved() {
        version.update { it + 1 }
    }

    /** The key is fine as of [atMs]: older refusals and credit exhaustion are stale, older failures will be ignored. */
    private fun proveGood(
        auth: AuthState.SignedIn,
        atMs: Long,
    ) {
        val key = auth.session.scope.key
        val token = auth.session.token
        var changed = false
        if (atMs > (goodAt.value[key] ?: Long.MIN_VALUE)) {
            goodAt.update { it.raised(key, atMs) }
            changed = true
        }
        if ((creditAt.value[token] ?: Long.MAX_VALUE) <= atMs) creditAt.update { it - token }
        if ((rejected.value[key] ?: Long.MAX_VALUE) <= atMs) {
            rejected.update { it - key }
            changed = true
        }
        if (changed) persist(key)
    }

    /** Writes what is known now for [key]: the last writer wins, so the order of the launches does not matter. */
    private fun persist(key: String) {
        appScope.launch {
            store.edit { prefs ->
                fun replace(
                    pref: Preferences.Key<Set<String>>,
                    at: Long?,
                ) {
                    val rest = prefs[pref].orEmpty().filterNot { it.substringAfter(':') == key }.toSet()
                    val next = if (at == null) rest else rest + "$at:$key"
                    if (next.isEmpty()) prefs.remove(pref) else prefs[pref] = next
                }
                replace(KEY_REJECTED_AT, rejected.value[key])
                replace(KEY_GOOD_AT, goodAt.value[key])
                replace(KEY_SEEN_AT, seenAt.value[key])
                val legacy = prefs[KEY_REJECTED].orEmpty() - key
                if (legacy.isEmpty()) prefs.remove(KEY_REJECTED) else prefs[KEY_REJECTED] = legacy
            }
        }
    }
}

/**
 * The last millisecond of the second of an HTTP `Date` (precision of one second). A failure instant has milliseconds, so
 * a failure inside the same second as the proof cannot be told from before or after it: it counts as before, the safe
 * side (a replayed old failure must not re-arm a state the user just fixed; a genuinely new failure within the same
 * second is missed until the next one).
 */
private fun Instant.endOfSecondMs(): Long = toEpochMilli().let { it - Math.floorMod(it, 1000L) + 999L }

private const val TAG = "LlmKeyMonitor"

/** How far ahead of the clocks a server `Date` may be before it is taken for a mistake. */
private const val MAX_DATE_AHEAD_MS = 24L * 60 * 60 * 1000
