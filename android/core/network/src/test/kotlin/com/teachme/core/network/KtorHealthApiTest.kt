package com.teachme.core.network

import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.UnreachableReason
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

class KtorHealthApiTest {
    private val jsonHeaders = headersOf(HttpHeaders.ContentType, "application/json")

    private class FakeSettings(
        initial: BackendSettings,
    ) : SettingsRepository {
        private val state = MutableStateFlow(initial)
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(
        settings: BackendSettings = BackendSettings("http://100.64.0.1:8080/", "secret-token"),
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData,
    ): KtorHealthApi {
        val client = createBackendHttpClient(MockEngine(handler), FakeSettings(settings), AppVersion("0.1.0"))
        return KtorHealthApi(client)
    }

    @Test
    fun `200 maps to connected and sends url and headers`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                api {
                    seen = it
                    respond(
                        content =
                            """{"status":"ok","version":"0.1.0","api_version":1,"catalog_version":1,
                    |"server_time":"2026-11-08T14:36:01.129Z","extra":"ignored"}
                            """.trimMargin(),
                        status = HttpStatusCode.OK,
                        headers = jsonHeaders,
                    )
                }

            val result = api.check()

            assertTrue(result is ConnectionResult.Connected)
            assertEquals(1, (result as ConnectionResult.Connected).health.catalogVersion)
            assertEquals("http://100.64.0.1:8080/api/v1/health", seen!!.url.toString())
            assertEquals("Bearer secret-token", seen.headers[HttpHeaders.Authorization])
            assertEquals("0.1.0", seen.headers["X-App-Version"])
        }

    @Test
    fun `base url with api prefix is normalized`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                api(BackendSettings("https://teach.example.ts.net/api/v1/", "t")) {
                    seen = it
                    respond(
                        """{"status":"ok","version":"1","api_version":1,"catalog_version":1,"server_time":"x"}""",
                        HttpStatusCode.OK,
                        jsonHeaders,
                    )
                }

            api.check()

            assertEquals("https://teach.example.ts.net/api/v1/health", seen!!.url.toString())
        }

    @Test
    fun `override settings win over stored ones`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                api {
                    seen = it
                    respond("", HttpStatusCode.Unauthorized, jsonHeaders)
                }

            val result = api.check(BackendSettings("http://10.0.0.5:9000", "typed-token"))

            assertEquals(ConnectionResult.SessionEnded, result)
            assertEquals("http://10.0.0.5:9000/api/v1/health", seen!!.url.toString())
            assertEquals("Bearer typed-token", seen.headers[HttpHeaders.Authorization])
        }

    @Test
    fun `401 with error body maps to invalid token`() =
        runTest {
            val api =
                api {
                    respond(
                        """{"error":{"code":"unauthorized","message":"Token non valido.","details":null,"request_id":"r1"}}""",
                        HttpStatusCode.Unauthorized,
                        jsonHeaders,
                    )
                }

            assertEquals(ConnectionResult.SessionEnded, api.check())
        }

    @Test
    fun `network failure maps to unreachable`() =
        runTest {
            val api = api { throw IOException("Connection refused") }

            assertEquals(ConnectionResult.Unreachable(), api.check())
        }

    @Test
    fun `blank base url maps to unreachable without a request`() =
        runTest {
            var called = false
            val api =
                api(BackendSettings("", "")) {
                    called = true
                    respond("", HttpStatusCode.OK)
                }

            assertEquals(ConnectionResult.Unreachable(), api.check())
            assertTrue(!called)
        }

    @Test
    fun `500 with error body maps to unexpected with code and parsed error`() =
        runTest {
            val api =
                api {
                    respond(
                        """{"error":{"code":"internal","message":"Errore interno.","details":null,"request_id":"r2"}}""",
                        HttpStatusCode.InternalServerError,
                        jsonHeaders,
                    )
                }

            val result = api.check() as ConnectionResult.Unexpected

            assertEquals(500, result.httpStatus)
            assertEquals("internal", result.error?.code)
            assertEquals("r2", result.error?.requestId)
        }

    @Test
    fun `500 with non json body maps to unexpected without error`() =
        runTest {
            val api = api { respond("<html>bad gateway</html>", HttpStatusCode.BadGateway) }

            val result = api.check() as ConnectionResult.Unexpected

            assertEquals(502, result.httpStatus)
            assertEquals(null, result.error)
        }

    @Test
    fun `200 with malformed body maps to unexpected`() =
        runTest {
            val api = api { respond("""{"hello":"world"}""", HttpStatusCode.OK, jsonHeaders) }

            assertEquals(ConnectionResult.Unexpected(200), api.check())
        }

    @Test
    fun `transport failures carry a reason`() =
        runTest {
            val cases =
                mapOf<Throwable, UnreachableReason>(
                    java.net.UnknownHostException("x") to UnreachableReason.UnknownHost,
                    java.net.ConnectException("refused") to UnreachableReason.ConnectionRefused,
                    java.net.SocketTimeoutException("slow") to UnreachableReason.Timeout,
                    javax.net.ssl.SSLHandshakeException("cert") to UnreachableReason.Tls,
                )
            for ((failure, reason) in cases) {
                assertEquals(ConnectionResult.Unreachable(reason), api { throw failure }.check())
            }
        }

    @Test
    fun `ktor timeouts map to timeout reason`() =
        runTest {
            assertEquals(
                ConnectionResult.Unreachable(UnreachableReason.Timeout),
                api {
                    throw io.ktor.client.plugins
                        .HttpRequestTimeoutException("http://x", 10_000)
                }.check(),
            )
            assertEquals(
                ConnectionResult.Unreachable(UnreachableReason.Timeout),
                api {
                    throw io.ktor.client.network.sockets
                        .ConnectTimeoutException("connect")
                }.check(),
            )
        }

    @Test
    fun `404 and 429 map to unexpected with the status code`() =
        runTest {
            for (code in listOf(HttpStatusCode.NotFound, HttpStatusCode.TooManyRequests)) {
                val body = """{"error":{"code":"x","message":"m","details":null,"request_id":"r"}}"""
                val result = api { respond(body, code, jsonHeaders) }.check() as ConnectionResult.Unexpected
                assertEquals(code.value, result.httpStatus)
                assertEquals("x", result.error?.code)
            }
        }

    @Test
    fun `redirect is not followed and maps to unexpected with the status code`() =
        runTest {
            var requests = 0
            val api =
                api {
                    requests++
                    respond(
                        "",
                        HttpStatusCode.Found,
                        headersOf(HttpHeaders.Location, "http://other.example/api/v1/health"),
                    )
                }

            val result = api.check()

            assertEquals(ConnectionResult.Unexpected(httpStatus = 302), result)
            assertEquals(1, requests)
        }

    @Test
    fun `uppercase scheme in stored url is normalized by the client`() =
        runTest {
            var seen: HttpRequestData? = null
            val api =
                api(BackendSettings("HTTP://100.64.0.1:8080", "t")) {
                    seen = it
                    respond("", HttpStatusCode.Unauthorized, jsonHeaders)
                }

            assertEquals(ConnectionResult.SessionEnded, api.check())
            assertEquals("http://100.64.0.1:8080/api/v1/health", seen!!.url.toString())
        }
}
