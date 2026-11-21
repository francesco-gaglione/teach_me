package com.teachme.core.model

import kotlinx.coroutines.flow.toList
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionModelTest {
    private val user = AuthUser("u1", "valentina", "Valentina", hasLlmKey = true, llmProvider = LlmProvider.OpenRouter)

    @Test
    fun `the token never appears when a session or settings are printed`() {
        val session = Session("https://teach.ts.net", "tm_secret-token-value", user)
        val settings = BackendSettings("https://teach.ts.net", "tm_secret-token-value", "u1")

        assertFalse(session.toString().contains("secret"))
        assertFalse(settings.toString().contains("secret"))
        assertFalse(LoginOutcome.Success("https://x", "tm_secret-token-value", user).toString().contains("secret"))
    }

    @Test
    fun `the scope of a session is its user on its address, spelled the same however it was typed`() {
        val a = Session("https://Teach.ts.net/", "t", user).scope
        val b = Session("https://Teach.ts.net/api/v1", "t2", user).scope

        assertEquals(a, b)
        assertEquals(a.key, b.key)
        // Another user on the same address, or the same user on another address, is another scope.
        assertTrue(a.key != UserScope("u2", "https://Teach.ts.net").key)
        assertTrue(a.key != UserScope("u1", "https://other.ts.net").key)
    }

    @Test
    fun `an unknown llm provider is tolerated`() {
        assertEquals(LlmProvider.Anthropic, LlmProvider.fromApi("anthropic"))
        assertEquals(LlmProvider.OpenRouter, LlmProvider.fromApi("openrouter"))
        assertEquals(LlmProvider.Unknown, LlmProvider.fromApi("some-new-one"))
        assertEquals(LlmProvider.Unknown, LlmProvider.fromApi(null))
    }

    @Test
    fun `login variants follow the reason the session is missing`() {
        assertEquals(
            SignedOutReason.entries.size,
            SignedOutReason.entries
                .map { it.name }
                .toSet()
                .size,
        )
    }

    @Test
    fun `address validation is the one of the old settings screen`() {
        assertNull(validateAddress("Http://100.64.0.1:8080"))
        assertNull(validateAddress("https://teach.tailnet.ts.net/api/v1/"))
        assertEquals(AddressError.Empty, validateAddress("   "))
        assertEquals(AddressError.InvalidScheme, validateAddress("teach.local"))
        assertEquals(AddressError.InvalidHost, validateAddress("http://"))
        assertEquals(AddressError.InvalidHost, validateAddress("http://10.0."))
    }

    @Test
    fun `the host is what the screens show`() {
        assertEquals("teach.tailnet.ts.net", hostOf("https://teach.tailnet.ts.net"))
        assertEquals("100.64.0.1:8080", hostOf("http://100.64.0.1:8080/api/v1"))
    }

    @Test
    fun `every move into signed in is a transition, also the same user coming back after a 401`() =
        kotlinx.coroutines.test.runTest {
            val a = Session("http://h", "t1", user)
            val states =
                listOf(
                    AuthState.Loading,
                    AuthState.SignedIn(a),
                    // The same session with fresh user data: not a transition.
                    AuthState.SignedIn(a.copy(user = user.copy(hasLlmKey = false))),
                    AuthState.SignedOut(SignedOutReason.Expired, "http://h", null),
                    AuthState.SignedIn(Session("http://h", "t2", user)),
                )

            val seen =
                kotlinx.coroutines.flow
                    .flowOf(*states.toTypedArray())
                    .signInTransitions()
                    .toList()

            assertEquals(listOf("t1", "t2"), seen.map { it.token })
        }
}
