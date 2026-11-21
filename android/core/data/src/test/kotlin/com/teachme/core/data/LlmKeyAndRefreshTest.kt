package com.teachme.core.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringSetPreferencesKey
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.UnreachableReason
import com.teachme.core.model.gradeBlocker
import com.teachme.core.network.AuthApi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private class FakeAuthApi : AuthApi {
    var me: ApiResult<AuthUser> = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
    var meCalls = 0

    override suspend fun login(
        baseUrl: String,
        username: String,
        password: String,
        deviceName: String?,
    ): LoginOutcome = error("not used")

    override suspend fun logout(
        baseUrl: String,
        token: String,
    ) = true

    override suspend fun me(): ApiResult<AuthUser> {
        meCalls++
        return me
    }

    override suspend fun changePassword(
        currentPassword: String,
        newPassword: String,
    ): ApiResult<Unit> = error("not used")

    override suspend fun setLlmKey(apiKey: String): ApiResult<AuthUser> = error("not used")

    override suspend fun removeLlmKey(): ApiResult<Unit> = error("not used")

    override suspend fun setTtsVoice(voice: String): ApiResult<AuthUser> = error("not used")
}

/** Counts the writes that reach the disk. */
private class CountingStore(
    private val inner: DataStore<Preferences>,
) : DataStore<Preferences> {
    @Volatile var edits = 0
    override val data: Flow<Preferences> = inner.data

    override suspend fun updateData(transform: suspend (t: Preferences) -> Preferences): Preferences {
        edits++
        return inner.updateData(transform)
    }
}

/** Its first read waits for [gate], like a slow disk; writes go straight through. */
private class GatedStore(
    private val inner: DataStore<Preferences>,
    val gate: CompletableDeferred<Unit>,
) : DataStore<Preferences> {
    override val data: Flow<Preferences> =
        flow {
            gate.await()
            emit(inner.data.first())
        }

    override suspend fun updateData(transform: suspend (t: Preferences) -> Preferences) = inner.updateData(transform)
}

private val goodAtPref = stringSetPreferencesKey("key_good_at")
private val rejectedAtPref = stringSetPreferencesKey("rejected_at")

internal class LlmKeyAndRefreshTest : DataStoreTest() {
    private val userWithKey = AuthUser("u1", "valentina", "Valentina", hasLlmKey = true)
    private val userNoKey = AuthUser("u2", "anna", "Anna", hasLlmKey = false)

    private suspend fun signedIn(user: AuthUser = userWithKey): DefaultSessionRepository {
        val repo = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
        repo.awaitReady()
        repo.signIn("http://teach.test", "t", user)
        return repo
    }

    private fun monitor(sessions: DefaultSessionRepository) =
        DefaultLlmKeyMonitor(sessions, store("llm_key"), newScope())

    private fun monitorAt(
        sessions: DefaultSessionRepository,
        nowMs: Long,
    ) = DefaultLlmKeyMonitor(sessions, store("llm_key"), newScope(), AppClock { nowMs })

    private fun at(second: Long) = java.time.Instant.ofEpochSecond(second)

    /** Waits for what the test expects on the disk: a real signal instead of a sleep. */
    private suspend fun DataStore<Preferences>.awaitDisk(condition: (Preferences) -> Boolean) =
        withTimeout(5_000) { data.first(condition) }

    private fun scopeKey(sessions: DefaultSessionRepository) =
        (sessions.state.value as AuthState.SignedIn).session.scope.key

    private suspend fun DefaultLlmKeyMonitor.await(condition: (LlmKeyStatus) -> Boolean) =
        withTimeout(5_000) {
            status.first(condition)
        }

    @Test
    fun `the state follows has_llm_key, and a missing key is told by the user`() =
        runBlocking<Unit> {
            val sessions = signedIn(userNoKey)
            val monitor = monitor(sessions)

            assertEquals(LlmKeyState.Missing, monitor.await { it.state == LlmKeyState.Missing }.state)
            sessions.updateUser(userNoKey.copy(hasLlmKey = true))
            assertEquals(LlmKeyState.Set, monitor.await { it.state == LlmKeyState.Set }.state)
        }

    @Test
    fun `a refusal is remembered for this user, survives a restart and goes when a key is saved`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val disk = store("llm_key")
            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope())
            monitor.await { it.state == LlmKeyState.Set }

            monitor.noteObserved(LlmKeyVariant.Rejected)
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            // Let the write reach the disk before the process dies.
            disk.awaitDisk { prefs -> prefs[rejectedAtPref].orEmpty().any { it.endsWith(":${scopeKey(sessions)}") } }

            killProcess()
            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope())
            assertEquals(LlmKeyState.RejectedKnown, again.await { it.state == LlmKeyState.RejectedKnown }.state)

            again.noteSaved()
            val saved = again.await { it.state == LlmKeyState.Set && it.version == 1 }
            assertEquals(1, saved.version)
        }

    @Test
    fun `a successful operation clears a stale refusal, even when has_llm_key never changed`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            monitor.noteWorked(at(200))

            // The key was replaced from the server: it works now, and the version is not touched.
            assertEquals(0, monitor.await { it.state == LlmKeyState.Set }.version)
        }

    @Test
    fun `an exhausted credit is not a refusal, leaves the version alone and clears an older refusal`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitorAt(sessions, nowMs = 0)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            monitor.noteObserved(LlmKeyVariant.Credit, at(200))

            // The service accepted the key: the refusal goes, the credit is remembered for this run only.
            val status = monitor.await { it.state == LlmKeyState.Set && it.creditExhaustedSeen }
            assertEquals(0, status.version)
            assertEquals(LlmKeyVariant.Credit, status.gradeBlocker)
            // Nothing suggests the key: no hint for the credit (llm-key.md 9.1).
            assertEquals(null, status.hintVariant)
        }

    @Test
    fun `an exhausted credit alone never marks the key as refused`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }

            monitor.noteObserved(LlmKeyVariant.Credit)
            monitor.await { it.creditExhaustedSeen }

            killProcess()
            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope())
            // After a restart nothing is left of it: not the flag, not a refusal.
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyState.Set, again.status.value.state)
            assertEquals(false, again.status.value.creditExhaustedSeen)
        }

    @Test
    fun `the credit flag goes at the first operation that works and is per user`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // Explicit, increasing instants: the real clock may give the same millisecond to two calls.
            val monitor = monitorAt(sessions, nowMs = 0)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Credit, at(100))
            monitor.await { it.creditExhaustedSeen }

            monitor.noteWorked(at(200))
            monitor.await { !it.creditExhaustedSeen }

            monitor.noteObserved(LlmKeyVariant.Credit, at(300))
            monitor.await { it.creditExhaustedSeen }
            sessions.signOut()
            sessions.signIn("http://teach.test", "t2", userNoKey.copy(hasLlmKey = true))
            assertEquals(false, monitor.await { it.state == LlmKeyState.Set }.creditExhaustedSeen)
        }

    @Test
    fun `signing out forgets the credit flag even for the same user signing in again`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitorAt(sessions, nowMs = 0)
            monitor.noteObserved(LlmKeyVariant.Credit, at(100))
            monitor.await { it.creditExhaustedSeen }

            sessions.signOut()
            // A real signal that the sign-out happened, instead of a sleep.
            withTimeout(5_000) { sessions.state.first { it is AuthState.SignedOut } }
            sessions.signIn("http://teach.test", "t3", userWithKey)

            assertEquals(false, monitor.await { it.state == LlmKeyState.Set }.creditExhaustedSeen)
        }

    @Test
    fun `a late report of user A does not land on user B`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }
            // The report is made while A is signed in...
            monitor.noteObserved(LlmKeyVariant.Rejected)
            // ...and B signs in right after, before the work behind it ran.
            sessions.signOut()
            sessions.signIn("http://teach.test", "t2", userNoKey.copy(hasLlmKey = true))

            // B is never shown as refused, whatever the order the work ran in.
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
        }

    @Test
    fun `a refusal of one user is not the refusal of another`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected)
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            sessions.signOut()
            sessions.signIn("http://teach.test", "t2", userNoKey.copy(hasLlmKey = true))

            assertEquals(LlmKeyState.Set, monitor.await { it.state == LlmKeyState.Set }.state)
        }

    @Test
    fun `a key reported missing makes the cached user keyless, and never lowers the version`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }

            monitor.noteObserved(LlmKeyVariant.Missing)

            monitor.await { it.state == LlmKeyState.Missing }
            assertEquals(false, (sessions.state.value as AuthState.SignedIn).session.user.hasLlmKey)
            // A panel born with this refusal must not look "fixed" at once: observing does not raise the version.
            assertEquals(0, monitor.status.value.version)
        }

    @Test
    fun `saving or removing a key raises the version, so panels know to move on`() =
        runBlocking<Unit> {
            val monitor = monitor(signedIn())
            monitor.noteRemoved()
            monitor.await { it.version == 1 }
            monitor.noteSaved()
            assertEquals(2, monitor.await { it.version == 2 }.version)
        }

    @Test
    fun `a failure not later than the last proof the key is fine is ignored, for every variant`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitor(sessions)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteWorked(at(100))

            // The same old failure may come from the stream, a reload, the polling...: nothing happens, any number of times.
            repeat(2) {
                monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
                monitor.noteObserved(LlmKeyVariant.Rejected, at(60))
                monitor.noteObserved(LlmKeyVariant.Missing, at(50))
                monitor.noteObserved(LlmKeyVariant.Credit, at(99))
            }
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyStatus(LlmKeyState.Set), monitor.status.value)
            assertTrue((sessions.state.value as AuthState.SignedIn).session.user.hasLlmKey)

            // A later failure counts, and is idempotent.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(101))
            monitor.noteObserved(LlmKeyVariant.Rejected, at(101))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `a failure from before a saved key is ignored, even after a restart`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val disk = store("llm_key")
            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope(), AppClock { 500_000 })
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            monitor.noteSaved()
            monitor.await { it.state == LlmKeyState.Set && it.version == 1 }
            // Let the write reach the disk before the process dies.
            disk.awaitDisk { it[goodAtPref] == setOf("500000:${scopeKey(sessions)}") && it[rejectedAtPref] == null }

            killProcess()
            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope())

            // The old panel, the replayed event, the reloaded job: all older than the key saved.
            again.noteObserved(LlmKeyVariant.Rejected, at(100))
            again.noteObserved(LlmKeyVariant.Rejected, at(499))
            again.noteObserved(LlmKeyVariant.Missing, at(300))
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyState.Set, again.status.value.state)
            assertTrue((reopened.state.value as AuthState.SignedIn).session.user.hasLlmKey)

            again.noteObserved(LlmKeyVariant.Rejected, at(501))
            again.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `a failure older than an operation that worked is ignored after a restart, a newer refusal stays`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val disk = store("llm_key")
            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope())
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            // An old success (a message page, a replayed event) does not clear a newer refusal...
            monitor.noteWorked(at(50))
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyState.RejectedKnown, monitor.status.value.state)
            // ...a later one does, and what was older than it is ignored from now on.
            monitor.noteWorked(at(200))
            monitor.await { it.state == LlmKeyState.Set }
            disk.awaitDisk {
                it[goodAtPref]?.any { e ->
                    e.startsWith("200000:")
                } == true &&
                    it[rejectedAtPref] == null
            }

            killProcess()
            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope())
            again.noteObserved(LlmKeyVariant.Rejected, at(150))
            kotlinx.coroutines.delay(300)
            assertEquals(LlmKeyState.Set, again.status.value.state)
        }

    @Test
    fun `an old credit exhaustion is ignored after a success, and a later one is kept`() =
        runBlocking<Unit> {
            val monitor = monitorAt(signedIn(), nowMs = 0)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteWorked(at(100))

            monitor.noteObserved(LlmKeyVariant.Credit, at(90))
            kotlinx.coroutines.delay(300)
            assertEquals(false, monitor.status.value.creditExhaustedSeen)

            monitor.noteObserved(LlmKeyVariant.Credit, at(110))
            monitor.await { it.creditExhaustedSeen }
            monitor.noteWorked(at(105))
            kotlinx.coroutines.delay(300)
            assertEquals(true, monitor.status.value.creditExhaustedSeen)
            monitor.noteWorked(at(120))
            monitor.await { !it.creditExhaustedSeen }
        }

    @Test
    fun `a refusal stored by the previous version is still read, and a saved key clears it`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val scope = (sessions.state.value as AuthState.SignedIn).session.scope.key
            val disk = store("llm_key")
            disk.edit {
                it[
                    androidx.datastore.preferences.core
                        .stringSetPreferencesKey("rejected_scopes"),
                ] =
                    setOf(scope)
            }

            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope(), AppClock { 1_000 })
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            monitor.noteSaved()
            monitor.await { it.state == LlmKeyState.Set }
        }

    private fun refusedWhenWorkedFirst(workedFirst: Boolean) =
        runBlocking<Unit> {
            val monitor = monitor(signedIn())
            monitor.await { it.state == LlmKeyState.Set }
            // A job that succeeded yesterday and one that failed today: the list may bring them in either order.
            val worked = { monitor.noteWorked(at(100)) }
            val failed = { monitor.noteObserved(LlmKeyVariant.Rejected, at(200)) }
            if (workedFirst) {
                worked()
                failed()
            } else {
                failed()
                worked()
            }
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            kotlinx.coroutines.delay(100)
            assertEquals(LlmKeyState.RejectedKnown, monitor.status.value.state)
        }

    @Test
    fun `a success of yesterday then a refusal of today ends refused`() = refusedWhenWorkedFirst(workedFirst = true)

    @Test
    fun `a refusal of today then a success of yesterday ends refused`() = refusedWhenWorkedFirst(workedFirst = false)

    @Test
    fun `a device clock behind the server cannot re-arm a failure seen before the key was saved`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // The device is two minutes behind: the failure is at server second 1000, the device says 880.
            var nowMs = 880_000L
            val monitor = DefaultLlmKeyMonitor(sessions, store("llm_key"), newScope(), AppClock { nowMs })
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_000))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            nowMs = 910_000L
            monitor.noteSaved()
            monitor.await { it.state == LlmKeyState.Set && it.version == 1 }
            // The reloaded old job, even one seen again later: it was seen before the save.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_000))
            monitor.noteObserved(LlmKeyVariant.Rejected, at(999))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)

            // A stale failure that was ignored still counts as seen, for a success with no server instant.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_050))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            monitor.noteWorkedNow()
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_050))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
            // A really new failure still arms it.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_051))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `a credit failure does not push a later save ahead of the device clock`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // The device is behind the server: the credit failed at server second 1000, the device says 880.
            var nowMs = 880_000L
            val monitor = DefaultLlmKeyMonitor(sessions, store("llm_key"), newScope(), AppClock { nowMs })
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Credit, at(1_000))
            monitor.await { it.creditExhaustedSeen }

            nowMs = 910_000L
            monitor.noteSaved()
            monitor.await { it.version == 1 }

            // The credit is not a key failure: the save is dated by the device (910s), so a refusal at 1000s is really new.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_000))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `a failure seen and ignored is remembered across a restart for a key saved later`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val disk = store("llm_key")
            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope(), AppClock { 100_000 })
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteWorked(at(2_000))
            // Ignored as stale, but seen.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_900))
            disk.awaitDisk { prefs -> prefs[stringSetPreferencesKey("key_failure_seen_at")] != null }
            killProcess()

            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope(), AppClock { 100_000 })
            again.noteSaved()
            again.await { it.version == 1 }
            again.noteObserved(LlmKeyVariant.Rejected, at(1_950))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, again.status.value.state)
        }

    @Test
    fun `an old credit failure read after a restart does not re-arm, a new one in the session does`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val first = DefaultLlmKeyMonitor(sessions, store("llm_key"), newScope(), AppClock { 1_000_000 })
            first.await { it.state == LlmKeyState.Set }
            // In the first run: failed after the start, so it counts.
            first.noteObserved(LlmKeyVariant.Credit, at(1_500))
            first.await { it.creditExhaustedSeen }
            killProcess()

            val reopened = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            reopened.awaitReady()
            reopened.signIn("http://teach.test", "t", userWithKey)
            val again = DefaultLlmKeyMonitor(reopened, store("llm_key"), newScope(), AppClock { 2_000_000 })
            again.await { it.state == LlmKeyState.Set }
            // The same job, read again from the server: from before this run.
            again.noteObserved(LlmKeyVariant.Credit, at(1_500))
            kotlinx.coroutines.delay(200)
            assertEquals(false, again.status.value.creditExhaustedSeen)

            again.noteObserved(LlmKeyVariant.Credit, at(2_500))
            again.await { it.creditExhaustedSeen }
        }

    @Test
    fun `a refusal that is already known is not written to the disk again`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val counting = CountingStore(store("llm_key"))
            val monitor = DefaultLlmKeyMonitor(sessions, counting, newScope())
            monitor.await { it.state == LlmKeyState.Set }

            monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            // Every poll and reload says the same, or something older.
            repeat(3) {
                monitor.noteObserved(LlmKeyVariant.Rejected, at(100))
                monitor.noteObserved(LlmKeyVariant.Rejected, at(90))
            }
            // A really new instant is the barrier: the writes are in order.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(200))
            counting.awaitDisk { prefs -> prefs[rejectedAtPref].orEmpty().any { it.startsWith("200000:") } }

            assertEquals(2, counting.edits)
        }

    @Test
    fun `a write made before the first read of the disk ends does not erase what the disk knows`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val key = scopeKey(sessions)
            val real = store("llm_key")
            real.edit { it[goodAtPref] = setOf("1000000:$key") }
            val gate = CompletableDeferred<Unit>()
            val monitor = DefaultLlmKeyMonitor(sessions, GatedStore(real, gate), newScope())

            // The disk is still being read: a refusal older than the proof on disk, and a success older than it too.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(500))
            monitor.noteWorked(at(600))
            kotlinx.coroutines.delay(200)
            assertEquals(setOf("1000000:$key"), real.data.first()[goodAtPref])

            gate.complete(Unit)
            // The refusal is older than the proof read from the disk: dropped. A later one is a barrier.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(2_000))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            real.awaitDisk { prefs -> prefs[rejectedAtPref].orEmpty().any { it.startsWith("2000000:") } }
            assertEquals(setOf("1000000:$key"), real.data.first()[goodAtPref])
            assertEquals(setOf("2000000:$key"), real.data.first()[rejectedAtPref])
        }

    @Test
    fun `a legacy refusal is dated at the migration, so an older success read from the server does not clear it`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val key = scopeKey(sessions)
            val disk = store("llm_key")
            disk.edit { it[stringSetPreferencesKey("rejected_scopes")] = setOf(key) }
            val monitor = DefaultLlmKeyMonitor(sessions, disk, newScope(), AppClock { 1_000_000 })
            monitor.await { it.state == LlmKeyState.RejectedKnown }
            // The migration fixes the date on disk for the next runs.
            disk.awaitDisk {
                it[rejectedAtPref] == setOf("1000000:$key") &&
                    it[stringSetPreferencesKey("rejected_scopes")] == null
            }

            monitor.noteWorked(at(500))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.RejectedKnown, monitor.status.value.state)
            monitor.noteWorked(at(1_500))
            monitor.await { it.state == LlmKeyState.Set }
        }

    // region GET /me

    private fun refresher(
        sessions: DefaultSessionRepository,
        api: FakeAuthApi,
        keys: DefaultLlmKeyMonitor,
        clock: () -> Long,
    ) = DefaultSessionRefresher(sessions, api, keys, AppClock { clock() })

    @Test
    fun `the foreground refresh asks at most once a minute, the one at Settings always`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val api = FakeAuthApi().apply { me = ApiResult.Success(userWithKey) }
            var now = 1_000L
            val refresher = refresher(sessions, api, monitor(sessions)) { now }

            refresher.refreshIfStale()
            now += 30_000
            refresher.refreshIfStale()
            assertEquals(1, api.meCalls)

            refresher.refresh()
            assertEquals(2, api.meCalls)

            now += 61_000
            refresher.refreshIfStale()
            assertEquals(3, api.meCalls)
        }

    @Test
    fun `a key set from the server shows up and raises the version`() =
        runBlocking<Unit> {
            val sessions = signedIn(userNoKey)
            val keys = monitor(sessions)
            val api = FakeAuthApi().apply { me = ApiResult.Success(userNoKey.copy(hasLlmKey = true)) }

            refresher(sessions, api, keys) { 0L }.refresh()

            assertTrue((sessions.state.value as AuthState.SignedIn).session.user.hasLlmKey)
            keys.await { it.state == LlmKeyState.Set && it.version == 1 }
        }

    @Test
    fun `a failed refresh keeps the cache`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val api = FakeAuthApi()

            refresher(sessions, api, monitor(sessions)) { 0L }.refresh()

            assertEquals(userWithKey, (sessions.state.value as AuthState.SignedIn).session.user)
        }

    @Test
    fun `nothing is asked without a session`() =
        runBlocking<Unit> {
            val sessions = DefaultSessionRepository(FakeSettings(), store("session"), RecordingClaimer(), newScope())
            sessions.awaitReady()
            val api = FakeAuthApi()

            refresher(sessions, api, monitor(sessions)) { 0L }.refresh()

            assertEquals(0, api.meCalls)
        }

    // region issue 19: the server's date, the session, the credit

    /** The device is far ahead of the server: a proof dated by the `Date` of the response still lets a new failure in. */
    @Test
    fun `a key saved with a server date is not hidden by a device clock that runs ahead`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // The device says second 1_000_000; the server says 1_000 (about 11 days apart).
            val monitor = monitorAt(sessions, nowMs = 1_000_000_000L)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(990))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            monitor.noteSaved(at(1_000))
            monitor.await { it.state == LlmKeyState.Set && it.version == 1 }
            // Same second as the proof (the Date has no milliseconds): counts as before it, the safe side.
            monitor.noteObserved(LlmKeyVariant.Rejected, java.time.Instant.ofEpochMilli(1_000_500))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
            // A later second is a really new failure, even if the device clock is a week ahead.
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_001))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `an operation that worked with a server date is not hidden by a device clock that runs ahead`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitorAt(sessions, nowMs = 1_000_000_000L)
            monitor.await { it.state == LlmKeyState.Set }

            monitor.noteWorkedAtServerDate(at(2_000))
            monitor.noteObserved(LlmKeyVariant.Credit, at(2_000))
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_999))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
            assertEquals(false, monitor.status.value.creditExhaustedSeen)

            monitor.noteObserved(LlmKeyVariant.Rejected, at(2_001))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `without a server date the device clock is still the fallback`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitorAt(sessions, nowMs = 5_000_000L)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteWorkedAtServerDate(null)
            monitor.noteObserved(LlmKeyVariant.Rejected, at(4_000))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
        }

    /** A session whose state the test sets by hand. */
    private class HandSessions(
        initial: AuthState,
    ) : com.teachme.core.model.SessionRepository {
        override val state = kotlinx.coroutines.flow.MutableStateFlow(initial)
        override val scope = kotlinx.coroutines.flow.flowOf<com.teachme.core.model.UserScope?>(null)

        override suspend fun current(): com.teachme.core.model.UserScope? = null

        override suspend fun awaitReady(): AuthState = state.value

        override suspend fun signIn(
            baseUrl: String,
            token: String,
            user: AuthUser,
        ) = error("not used")

        override suspend fun signOut() = error("not used")

        override suspend fun expire(token: String) = error("not used")

        override suspend fun updateUser(user: AuthUser) = error("not used")
    }

    @Test
    fun `a sign-out and a sign-in too close for the collector still forget the credit`() =
        runBlocking<Unit> {
            fun session(token: String) =
                AuthState.SignedIn(
                    com.teachme.core.model
                        .Session("http://teach.test", token, userWithKey),
                )
            val sessions = HandSessions(session("t1"))
            val executor =
                java.util.concurrent.Executors
                    .newSingleThreadExecutor()
            val scope =
                kotlinx.coroutines.CoroutineScope(
                    kotlinx.coroutines.SupervisorJob() + executor.asCoroutineDispatcher(),
                )
            try {
                val monitor = DefaultLlmKeyMonitor(sessions, store("llm_key"), scope, AppClock { 0 })
                monitor.await { it.state == LlmKeyState.Set }
                monitor.noteObserved(LlmKeyVariant.Credit, at(100))
                monitor.await { it.creditExhaustedSeen }

                // The only thread of the monitor is busy: it sees neither the exit nor the new sign-in as they happen.
                val release = java.util.concurrent.CountDownLatch(1)
                val busy = java.util.concurrent.CountDownLatch(1)
                executor.execute {
                    busy.countDown()
                    release.await()
                }
                busy.await()
                sessions.state.value =
                    AuthState.SignedOut(com.teachme.core.model.SignedOutReason.LoggedOut, "http://teach.test", null)
                sessions.state.value = session("t2")
                release.countDown()

                // The new session starts clean, whatever the collector saw.
                monitor.await { !it.creditExhaustedSeen }
                kotlinx.coroutines.delay(200)
                assertEquals(false, monitor.status.value.creditExhaustedSeen)
            } finally {
                scope.cancel()
                executor.shutdownNow()
            }
        }

    /** A failed answer of a request, as the screens report it, with the `Date` it carried. */
    private fun failedAnswer(
        failure: ApiFailure,
        date: java.time.Instant?,
    ) = ApiResult.Failure(failure, date)

    @Test
    fun `a refusal answered by a request is dated by the server, with a device clock that runs behind`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // The device is ten minutes behind the server: the Date of every answer is ahead of the device clock.
            val server =
                java.time.Instant
                    .now()
                    .plusSeconds(600)
                    .truncatedTo(java.time.temporal.ChronoUnit.SECONDS)
            val monitor = monitor(sessions)
            val notices =
                com.teachme.core.model
                    .KeyNoticeSource(monitor)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteSaved(server)
            monitor.await { it.version == 1 }

            // Two seconds later on the server clock, but still in the device's past: dated by the device it would be ignored.
            notices.of(failedAnswer(ApiFailure.LlmKeyRejected, server.plusSeconds(2)))
            monitor.await { it.state == LlmKeyState.RejectedKnown }
        }

    @Test
    fun `a credit answered by a request is cleared by a later success, with a device clock that runs ahead`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            // The device is ten minutes ahead of the server.
            val server =
                java.time.Instant
                    .now()
                    .minusSeconds(600)
                    .truncatedTo(java.time.temporal.ChronoUnit.SECONDS)
            val monitor = monitor(sessions)
            val notices =
                com.teachme.core.model
                    .KeyNoticeSource(monitor)
            monitor.await { it.state == LlmKeyState.Set }

            notices.of(failedAnswer(ApiFailure.LlmCreditExhausted, server))
            monitor.await { it.creditExhaustedSeen }
            // Dated by the device clock the failure would be ten minutes in the future and this success could not clear it.
            monitor.noteWorked(server.plusSeconds(5))
            monitor.await { !it.creditExhaustedSeen }
        }

    @Test
    fun `a failure and a proof in the same second of the server count as the failure being older`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val server =
                java.time.Instant
                    .now()
                    .truncatedTo(java.time.temporal.ChronoUnit.SECONDS)
            val monitor = monitor(sessions)
            val notices =
                com.teachme.core.model
                    .KeyNoticeSource(monitor)
            monitor.await { it.state == LlmKeyState.Set }
            monitor.noteSaved(server)
            monitor.await { it.version == 1 }

            notices.of(failedAnswer(ApiFailure.LlmKeyRejected, server))
            kotlinx.coroutines.delay(200)
            assertEquals(LlmKeyState.Set, monitor.status.value.state)
        }

    @Test
    fun `a Date far in the future is ignored, so it cannot hide real failures for ever`() =
        runBlocking<Unit> {
            val sessions = signedIn()
            val monitor = monitorAt(sessions, nowMs = 1_000_000L)
            monitor.await { it.state == LlmKeyState.Set }

            // A wrong proxy says 2099: the proof falls back to the device clock instead of blocking everything.
            monitor.noteSaved(java.time.Instant.parse("2099-01-01T00:00:00Z"))
            monitor.await { it.version == 1 }
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_001))
            monitor.await { it.state == LlmKeyState.RejectedKnown }

            monitor.noteWorkedAtServerDate(java.time.Instant.parse("2099-01-01T00:00:00Z"))
            monitor.noteObserved(LlmKeyVariant.Rejected, at(1_002))
            monitor.noteWorkedAtServerDate(at(1_500))
            monitor.await { it.state == LlmKeyState.Set }
        }

    // endregion
}
