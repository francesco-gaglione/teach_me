package com.teachme.core.data

import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.LlmProvider
import com.teachme.core.model.SignedOutReason
import com.teachme.core.model.UserScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

internal class SessionRepositoryTest : DataStoreTest() {
    private val user = AuthUser("u1", "valentina", "Valentina", hasLlmKey = true, llmProvider = LlmProvider.OpenRouter)
    private val other = AuthUser("u2", "anna", "Anna", hasLlmKey = false)

    private fun repo(
        settings: FakeSettings = FakeSettings(),
        claimer: RecordingClaimer = RecordingClaimer(),
    ) = DefaultSessionRepository(settings, store("session"), claimer, newScope())

    @Test
    fun `an install that never had an address starts at the first access`() =
        runTest {
            val state = repo().awaitReady() as AuthState.SignedOut

            assertEquals(SignedOutReason.FirstRun, state.reason)
            assertNull(state.user)
        }

    @Test
    fun `the single-user install with its address and static token is an update, and the old token is dropped`() =
        runTest {
            val settings = FakeSettings(BackendSettings("http://teach.test", "the-old-static-token"))

            val state = repo(settings).awaitReady() as AuthState.SignedOut

            assertEquals(SignedOutReason.Upgrade, state.reason)
            assertEquals("http://teach.test", state.baseUrl)
            assertEquals("", settings.state.value.token)
            assertEquals("http://teach.test", settings.state.value.baseUrl)
        }

    @Test
    fun `signing in saves the session, and a new process finds it again`() =
        runTest {
            val settings = FakeSettings()
            val first = repo(settings)
            first.awaitReady()
            first.signIn("http://teach.test/", "tm_token", user)

            val state = first.state.value as AuthState.SignedIn
            assertEquals("tm_token", state.session.token)
            assertEquals("http://teach.test", state.session.baseUrl)
            assertEquals(BackendSettings("http://teach.test", "tm_token", "u1"), settings.state.value)

            killProcess()
            val second = DefaultSessionRepository(settings, store("session"), RecordingClaimer(), newScope())
            val restored = (second.awaitReady() as AuthState.SignedIn).session
            assertEquals(user, restored.user)
            assertEquals(UserScope("u1", "http://teach.test"), second.current())
        }

    @Test
    fun `the data of the single-user install goes to the first user who signs in`() =
        runTest {
            val claimer = RecordingClaimer()
            val repo = repo(claimer = claimer)
            repo.awaitReady()

            repo.signIn("http://teach.test", "t", user)

            assertEquals(listOf(UserScope("u1", "http://teach.test")), claimer.claims)
        }

    @Test
    fun `signing out removes the token and keeps the address, the user and their data scope`() =
        runTest {
            val settings = FakeSettings()
            val repo = repo(settings)
            repo.awaitReady()
            repo.signIn("http://teach.test", "t", user)

            repo.signOut()

            val state = repo.state.value as AuthState.SignedOut
            assertEquals(SignedOutReason.LoggedOut, state.reason)
            assertEquals("http://teach.test", state.baseUrl)
            assertEquals("valentina", state.user?.username)
            assertEquals(BackendSettings("http://teach.test"), settings.state.value)
            // What they left in the queues is still theirs.
            assertEquals(UserScope("u1", "http://teach.test"), repo.current())

            killProcess()
            val again = DefaultSessionRepository(settings, store("session"), RecordingClaimer(), newScope())
            val restored = again.awaitReady() as AuthState.SignedOut
            assertEquals(SignedOutReason.LoggedOut, restored.reason)
            assertEquals("valentina", restored.user?.username)
        }

    @Test
    fun `a 401 ends only the session it was about`() =
        runTest {
            val repo = repo()
            repo.awaitReady()
            repo.signIn("http://teach.test", "new-token", user)

            // A late answer to a request of an older session.
            assertFalse(repo.expire("old-token"))
            assertTrue(repo.state.value is AuthState.SignedIn)

            assertTrue(repo.expire("new-token"))
            val state = repo.state.value as AuthState.SignedOut
            assertEquals(SignedOutReason.Expired, state.reason)
            assertEquals("u1", state.user?.id)
            // Several requests answer 401 together: the second one finds nothing left to end.
            assertFalse(repo.expire("new-token"))
        }

    @Test
    fun `an ended session is remembered across a restart as ended`() =
        runTest {
            val settings = FakeSettings()
            val repo = repo(settings)
            repo.awaitReady()
            repo.signIn("http://teach.test", "t", user)
            repo.expire("t")

            killProcess()
            val state =
                DefaultSessionRepository(
                    settings,
                    store("session"),
                    RecordingClaimer(),
                    newScope(),
                ).awaitReady()

            assertEquals(SignedOutReason.Expired, (state as AuthState.SignedOut).reason)
        }

    @Test
    fun `fresh user data replaces the cache only for the signed-in user`() =
        runTest {
            val repo = repo()
            repo.awaitReady()
            repo.signIn("http://teach.test", "t", user.copy(hasLlmKey = false))

            repo.updateUser(user)
            assertTrue((repo.state.value as AuthState.SignedIn).session.user.hasLlmKey)

            repo.updateUser(other)
            assertEquals("u1", (repo.state.value as AuthState.SignedIn).session.user.id)
        }

    @Test
    fun `the read-aloud voice is kept with the user and can go back to none`() =
        runTest {
            val repo = repo()
            repo.awaitReady()
            repo.signIn("http://teach.test", "t", user)
            assertNull((repo.state.value as AuthState.SignedIn).session.user.ttsVoice)

            repo.updateUser(user.copy(ttsVoice = "serena"))
            assertEquals("serena", (repo.state.value as AuthState.SignedIn).session.user.ttsVoice)

            repo.updateUser(user.copy(ttsVoice = null))
            assertNull((repo.state.value as AuthState.SignedIn).session.user.ttsVoice)
        }

    @Test
    fun `another user signing in changes the scope of the local data`() =
        runTest {
            val repo = repo()
            repo.awaitReady()
            repo.signIn("http://teach.test", "t1", user)
            val first = repo.current()
            repo.signOut()
            repo.signIn("http://teach.test", "t2", other)

            assertNotEquals(first, repo.current())
            assertEquals(UserScope("u2", "http://teach.test"), repo.scope.first())
        }

    @Test
    fun `a token without a matching user is not a session`() =
        runTest {
            val settings = FakeSettings(BackendSettings("http://teach.test", "token-of-nobody", "ghost"))

            val state = repo(settings).awaitReady()

            assertTrue(state is AuthState.SignedOut)
            assertEquals("", settings.state.value.token)
        }

    @Test
    fun `the session survives profile data written by an older run`() =
        runTest {
            val profile = store("session")
            profile.edit { it[stringPreferencesKey("user_id")] = "u1" }
            val settings = FakeSettings(BackendSettings("http://teach.test", "t", "u1"))

            val state = DefaultSessionRepository(settings, profile, RecordingClaimer(), newScope()).awaitReady()

            assertEquals("u1", (state as AuthState.SignedIn).session.user.id)
        }
}
