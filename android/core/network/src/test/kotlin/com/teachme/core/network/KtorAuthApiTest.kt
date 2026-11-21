package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ExpectedUserScope
import com.teachme.core.model.LlmProvider
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UnreachableReason
import com.teachme.core.model.UserScope
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.TextContent
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

class KtorAuthApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")

    private class FakeSettings(
        initial: BackendSettings,
    ) : SettingsRepository {
        val state = MutableStateFlow(initial)
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private val signedIn = BackendSettings("http://10.0.2.2:8080", "session-token", "u1")

    private fun api(
        settings: BackendSettings = signedIn,
        onUnauthorized: UnauthorizedHandler = UnauthorizedHandler.None,
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData,
    ) = KtorAuthApi(
        createBackendHttpClient(MockEngine(handler), FakeSettings(settings), AppVersion("0.1.0"), onUnauthorized),
    )

    private val userJson =
        """{"id":"u1","username":"valentina","display_name":"Valentina","has_llm_key":true,"llm_provider":"openrouter",
            |"created_at":"2026-11-10T09:12:44.031Z","brand_new_field":1}
        """.trimMargin()

    private fun error(
        code: String,
        details: String = "null",
    ) = """{"error":{"code":"$code","message":"x","details":$details,"request_id":"r1"}}"""

    private val health = """{"status":"ok"}"""

    private fun TextContent.body() = text

    // region login

    @Test
    fun `login checks the address, then signs in without any token, and returns what the backend said`() =
        runTest {
            val seen = mutableListOf<HttpRequestData>()
            val api =
                api(BackendSettings()) {
                    seen += it
                    if (it.url.encodedPath == "/health") {
                        respond(health, HttpStatusCode.OK, json)
                    } else {
                        respond(
                            """{"token":"tm_abc","expires_at":"2027-02-08T09:12:44.031Z","user":$userJson}""",
                            HttpStatusCode.OK,
                            json,
                        )
                    }
                }

            val outcome = api.login("HTTP://10.0.2.2:8080/", "Valentina", "pw-123456789", "Samsung SM-X710")

            val success = outcome as LoginOutcome.Success
            assertEquals("http://10.0.2.2:8080", success.baseUrl)
            assertEquals("tm_abc", success.token)
            assertEquals("valentina", success.user.username)
            assertTrue(success.user.hasLlmKey)
            assertEquals(LlmProvider.OpenRouter, success.user.llmProvider)
            assertEquals(listOf("/health", "/api/v1/auth/login"), seen.map { it.url.encodedPath })
            assertEquals(HttpMethod.Post, seen[1].method)
            assertNull(seen[0].headers[HttpHeaders.Authorization])
            assertNull(seen[1].headers[HttpHeaders.Authorization])
            val body = (seen[1].body as TextContent).body()
            assertTrue(body.contains("\"username\":\"Valentina\""))
            assertTrue(body.contains("\"device_name\":\"Samsung SM-X710\""))
        }

    @Test
    fun `an unknown provider and unknown fields do not break the login`() =
        runTest {
            val api =
                api(BackendSettings()) {
                    if (it.url.encodedPath == "/health") {
                        respond(health, HttpStatusCode.OK, json)
                    } else {
                        respond(
                            """{"token":"t","user":{"id":"u","username":"a","display_name":"A","has_llm_key":false,
                                |"llm_provider":"some-new-service","future":{}}}
                            """.trimMargin(),
                            HttpStatusCode.OK,
                            json,
                        )
                    }
                }

            val outcome = api.login("http://h", "a", "p", null) as LoginOutcome.Success

            assertEquals(LlmProvider.Unknown, outcome.user.llmProvider)
            assertFalse(outcome.user.hasLlmKey)
        }

    private suspend fun loginWith(loginAnswer: suspend MockRequestHandleScope.() -> HttpResponseData): LoginOutcome =
        api(BackendSettings()) {
            if (it.url.encodedPath == "/health") respond(health, HttpStatusCode.OK, json) else loginAnswer()
        }.login("http://h", "a", "p", null)

    @Test
    fun `wrong credentials are invalid credentials, not a session ending`() =
        runTest {
            var handled = false
            val api =
                api(BackendSettings(), onUnauthorized = { handled = true }) {
                    if (it.url.encodedPath == "/health") {
                        respond(health, HttpStatusCode.OK, json)
                    } else {
                        respond(error("invalid_credentials"), HttpStatusCode.Unauthorized, json)
                    }
                }

            assertEquals(LoginOutcome.InvalidCredentials, api.login("http://h", "a", "p", null))
            // A 401 at login is not the end of a session: no token was sent, the global handler stays out of it.
            assertFalse(handled)
        }

    @Test
    fun `too many attempts carry the retry-after seconds`() =
        runTest {
            val outcome =
                loginWith {
                    respond(
                        error("rate_limited"),
                        HttpStatusCode.TooManyRequests,
                        headersOf(
                            HttpHeaders.ContentType to listOf("application/json"),
                            "Retry-After" to listOf("840"),
                        ),
                    )
                }

            assertEquals(LoginOutcome.RateLimited(840), outcome)
        }

    @Test
    fun `a missing or non numeric retry-after is unknown`() =
        runTest {
            assertEquals(
                LoginOutcome.RateLimited(null),
                loginWith { respond(error("rate_limited"), HttpStatusCode.TooManyRequests, json) },
            )
            assertEquals(
                LoginOutcome.RateLimited(null),
                loginWith {
                    respond(
                        error("rate_limited"),
                        HttpStatusCode.TooManyRequests,
                        headersOf(
                            HttpHeaders.ContentType to listOf("application/json"),
                            "Retry-After" to listOf("soon"),
                        ),
                    )
                },
            )
        }

    @Test
    fun `an unavailable backend is not ready`() =
        runTest {
            assertEquals(
                LoginOutcome.NotReady,
                loginWith {
                    respond(error("unavailable"), HttpStatusCode.ServiceUnavailable, json)
                },
            )
        }

    @Test
    fun `a backend without the login route, or not teach_me at all, is not recognized`() =
        runTest {
            assertEquals(
                LoginOutcome.NotRecognized,
                loginWith { respond(error("not_found"), HttpStatusCode.NotFound, json) },
            )
            assertEquals(LoginOutcome.NotRecognized, loginWith { respond("<html>", HttpStatusCode.NotFound) })
            assertEquals(LoginOutcome.NotRecognized, loginWith { respond("", HttpStatusCode.MethodNotAllowed) })
            // A 200 that is not the documented session.
            assertEquals(
                LoginOutcome.NotRecognized,
                loginWith { respond("""{"hello":"world"}""", HttpStatusCode.OK, json) },
            )
        }

    @Test
    fun `an address that does not answer the liveness check is not recognized, and no login is tried`() =
        runTest {
            var loginTried = false
            val notJson =
                api(BackendSettings()) {
                    if (it.url.encodedPath == "/health") {
                        respond("<html>hi</html>", HttpStatusCode.OK)
                    } else {
                        loginTried = true
                        respond("", HttpStatusCode.OK)
                    }
                }
            assertEquals(LoginOutcome.NotRecognized, notJson.login("http://h", "a", "p", null))
            val gone = api(BackendSettings()) { respond("", HttpStatusCode.NotFound) }
            assertEquals(LoginOutcome.NotRecognized, gone.login("http://h", "a", "p", null))
            val notReady = api(BackendSettings()) { respond("", HttpStatusCode.ServiceUnavailable) }
            assertEquals(LoginOutcome.NotReady, notReady.login("http://h", "a", "p", null))
            assertFalse(loginTried)
        }

    @Test
    fun `server errors are unexpected and carry the status and the request id`() =
        runTest {
            assertEquals(
                LoginOutcome.Unexpected(500, "r1"),
                loginWith { respond(error("internal"), HttpStatusCode.InternalServerError, json) },
            )
            assertEquals(
                LoginOutcome.Unexpected(422, "r1"),
                loginWith { respond(error("validation_failed"), HttpStatusCode.UnprocessableEntity, json) },
            )
            assertEquals(
                LoginOutcome.Unexpected(502, null),
                loginWith { respond("bad gateway", HttpStatusCode.BadGateway) },
            )
        }

    @Test
    fun `transport failures at login say why the address cannot be reached`() =
        runTest {
            val cases =
                mapOf<Throwable, UnreachableReason>(
                    java.net.UnknownHostException("x") to UnreachableReason.UnknownHost,
                    java.net.ConnectException("refused") to UnreachableReason.ConnectionRefused,
                    java.net.SocketTimeoutException("slow") to UnreachableReason.Timeout,
                    javax.net.ssl.SSLHandshakeException("cert") to UnreachableReason.Tls,
                    IOException("other") to UnreachableReason.Other,
                )
            for ((failure, reason) in cases) {
                val outcome = api(BackendSettings()) { throw failure }.login("http://h", "a", "p", null)
                assertEquals(LoginOutcome.Unreachable(reason), outcome)
            }
        }

    // endregion

    // region the rest of users.md

    @Test
    fun `a success carries the Date of the response, and none when it is missing or invalid`() =
        runTest {
            val dated =
                api {
                    respond(
                        userJson,
                        HttpStatusCode.OK,
                        headersOf(
                            HttpHeaders.ContentType to listOf("application/json"),
                            HttpHeaders.Date to listOf("Sun, 15 Nov 2026 10:20:30 GMT"),
                        ),
                    )
                }.me()
            assertEquals(java.time.Instant.parse("2026-11-15T10:20:30Z"), (dated as ApiResult.Success).serverDate)

            val none = api { respond(userJson, HttpStatusCode.OK, json) }.me()
            assertNull((none as ApiResult.Success).serverDate)

            val bad =
                api {
                    respond(
                        userJson,
                        HttpStatusCode.OK,
                        headersOf(
                            HttpHeaders.ContentType to listOf("application/json"),
                            HttpHeaders.Date to listOf("yesterday"),
                        ),
                    )
                }.me()
            assertNull((bad as ApiResult.Success).serverDate)
        }

    @Test
    fun `me reads the user with its key flag`() =
        runTest {
            var seen: HttpRequestData? = null
            val result =
                api {
                    seen = it
                    respond(userJson, HttpStatusCode.OK, json)
                }.me()

            assertEquals("valentina", (result as ApiResult.Success).value.username)
            assertEquals("/api/v1/me", seen!!.url.encodedPath)
            assertEquals("Bearer session-token", seen.headers[HttpHeaders.Authorization])
        }

    @Test
    fun `changing the password sends both and reads the 422 details`() =
        runTest {
            var body = ""
            val ok =
                api {
                    body = (it.body as TextContent).text
                    assertEquals(HttpMethod.Put, it.method)
                    assertEquals("/api/v1/me/password", it.url.encodedPath)
                    respond("", HttpStatusCode.NoContent)
                }.changePassword("old-password", "new-password-123")
            assertTrue(ok is ApiResult.Success)
            assertTrue(body.contains("\"current_password\":\"old-password\""))
            assertTrue(body.contains("\"new_password\":\"new-password-123\""))

            val wrong =
                api {
                    respond(
                        error("validation_failed", """[{"field":"current_password","issue":"wrong"}]"""),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }.changePassword("x", "y")
            val failure = (wrong as ApiResult.Failure).failure as ApiFailure.Validation
            assertTrue(failure.hasIssue("current_password", "wrong"))
        }

    @Test
    fun `the llm key is written and removed, and the answer says only whether there is one`() =
        runTest {
            var body = ""
            val saved =
                api {
                    body = (it.body as TextContent).text
                    assertEquals("/api/v1/me/llm-key", it.url.encodedPath)
                    respond(userJson, HttpStatusCode.OK, json)
                }.setLlmKey("sk-or-secret")
            assertTrue((saved as ApiResult.Success).value.hasLlmKey)
            assertEquals("""{"api_key":"sk-or-secret"}""", body)

            var method: HttpMethod? = null
            val removed =
                api {
                    method = it.method
                    respond("", HttpStatusCode.NoContent)
                }.removeLlmKey()
            assertTrue(removed is ApiResult.Success)
            assertEquals(HttpMethod.Delete, method)
        }

    @Test
    fun `setting the voice puts it and reads tts_voice, null or missing included`() =
        runTest {
            var body = ""
            var method: HttpMethod? = null
            val saved =
                api {
                    method = it.method
                    body = (it.body as TextContent).text
                    assertEquals("/api/v1/me/tts-voice", it.url.encodedPath)
                    respond(userJson.replace("}", ""","tts_voice":"serena"}"""), HttpStatusCode.OK, json)
                }.setTtsVoice("serena")
            assertEquals("serena", (saved as ApiResult.Success).value.ttsVoice)
            assertEquals("""{"voice":"serena"}""", body)
            assertEquals(HttpMethod.Put, method)

            val without = api { respond(userJson, HttpStatusCode.OK, json) }.setTtsVoice("serena")
            assertNull((without as ApiResult.Success).value.ttsVoice)
            val off =
                api {
                    respond(
                        userJson.replace("}", ""","tts_voice":null}"""),
                        HttpStatusCode.OK,
                        json,
                    )
                }.setTtsVoice("x")
            assertNull((off as ApiResult.Success).value.ttsVoice)
        }

    @Test
    fun `logout is best effort and a 401 there is not news`() =
        runTest {
            var handled = false
            var auth: String? = null
            val api =
                api(BackendSettings(), onUnauthorized = { handled = true }) {
                    auth = it.headers[HttpHeaders.Authorization]
                    respond(error("unauthorized"), HttpStatusCode.Unauthorized, json)
                }

            assertFalse(api.logout("http://10.0.2.2:8080", "old-token"))
            assertEquals("Bearer old-token", auth)
            assertFalse(handled)
            assertFalse(api(BackendSettings()) { throw IOException("offline") }.logout("http://h", "t"))
            assertTrue(api(BackendSettings()) { respond("", HttpStatusCode.NoContent) }.logout("http://h", "t"))
        }

    // endregion

    // region the global rules of the client

    @Test
    fun `a 401 on a request that carried a token reaches the single handler with that token`() =
        runTest {
            val tokens = mutableListOf<String>()
            val api =
                api(
                    onUnauthorized = { tokens += it },
                ) { respond(error("unauthorized"), HttpStatusCode.Unauthorized, json) }

            val result = api.me()

            assertEquals(ApiResult.Failure(ApiFailure.Unauthorized), result)
            assertEquals(listOf("session-token"), tokens)
        }

    @Test
    fun `other errors do not reach the handler`() =
        runTest {
            val tokens = mutableListOf<String>()
            val api =
                api(
                    onUnauthorized = { tokens += it },
                ) { respond(error("internal"), HttpStatusCode.InternalServerError, json) }

            api.me()

            assertTrue(tokens.isEmpty())
        }

    @Test
    fun `work for one user is not sent with the session of another`() =
        runTest {
            var sent = false
            val api =
                api {
                    sent = true
                    respond(userJson, HttpStatusCode.OK, json)
                }

            val other = withContext(ExpectedUserScope(UserScope("u2", "http://10.0.2.2:8080"))) { api.me() }
            val mine = withContext(ExpectedUserScope(UserScope("u1", "http://10.0.2.2:8080/"))) { api.me() }

            // Refused before any request, as if the network were down: the queue keeps what it has.
            assertEquals(ApiResult.Failure(ApiFailure.Network(UnreachableReason.Other)), other)
            assertTrue(mine is ApiResult.Success)
            assertTrue(sent)
        }

    @Test
    fun `the key and rate limit error codes become their own failures`() =
        runTest {
            suspend fun failureOf(
                status: HttpStatusCode,
                code: String,
            ) = (api { respond(error(code), status, json) }.me() as ApiResult.Failure).failure

            assertEquals(ApiFailure.LlmKeyMissing, failureOf(HttpStatusCode.Conflict, "llm_key_missing"))
            assertEquals(ApiFailure.LlmKeyRejected, failureOf(HttpStatusCode.BadGateway, "llm_key_rejected"))
            assertEquals(ApiFailure.LlmCreditExhausted, failureOf(HttpStatusCode.BadGateway, "llm_credit_exhausted"))
            // The code decides together with the status: a 502 and a different code stays a plain failure.
            assertEquals(
                502,
                (failureOf(HttpStatusCode.BadGateway, "llm_key_rejected_x") as ApiFailure.Unexpected).httpStatus,
            )
            // A plain 409 and a plain 502 are what they were.
            assertEquals(ApiFailure.Conflict, failureOf(HttpStatusCode.Conflict, "conflict"))
            assertEquals(
                ApiFailure.Unexpected(502, null).httpStatus,
                (failureOf(HttpStatusCode.BadGateway, "upstream_failed") as ApiFailure.Unexpected).httpStatus,
            )
            assertEquals(ApiFailure.Unavailable, failureOf(HttpStatusCode.ServiceUnavailable, "unavailable"))
            assertEquals(ApiFailure.RateLimited(null), failureOf(HttpStatusCode.TooManyRequests, "rate_limited"))
        }

    // endregion
}
