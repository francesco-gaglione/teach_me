package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.get
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** The two codes added by audit C8/M1 (api-conventions section 7): `llm_budget_exceeded` and `storage_quota_exceeded`. */
class ApiCallBudgetAndQuotaTest {
    private fun failureOf(
        status: HttpStatusCode,
        code: String?,
        retryAfter: String? = null,
    ): ApiFailure =
        runBlocking {
            val body = code?.let { """{"error":{"code":"$it","message":"x"}}""" } ?: "not json"
            val headers =
                buildList {
                    add(HttpHeaders.ContentType to listOf("application/json"))
                    retryAfter?.let { add("Retry-After" to listOf(it)) }
                }
            val client = HttpClient(MockEngine { respond(body, status, headersOf(*headers.toTypedArray())) })
            client.get("http://localhost/x").toFailure()
        }

    @Test
    fun `budget exceeded carries Retry-After`() {
        assertEquals(
            ApiFailure.LlmBudgetExceeded(3600),
            failureOf(HttpStatusCode.TooManyRequests, "llm_budget_exceeded", "3600"),
        )
    }

    @Test
    fun `budget exceeded without a numeric Retry-After has none`() {
        assertEquals(
            ApiFailure.LlmBudgetExceeded(null),
            failureOf(HttpStatusCode.TooManyRequests, "llm_budget_exceeded"),
        )
        assertEquals(
            ApiFailure.LlmBudgetExceeded(null),
            failureOf(HttpStatusCode.TooManyRequests, "llm_budget_exceeded", "soon"),
        )
    }

    @Test
    fun `budget exceeded is not the busy rate limit`() {
        assertEquals(ApiFailure.RateLimited(60), failureOf(HttpStatusCode.TooManyRequests, "rate_limited", "60"))
    }

    @Test
    fun `storage quota is its own failure, not a plain conflict`() {
        assertEquals(ApiFailure.StorageQuotaExceeded, failureOf(HttpStatusCode.Conflict, "storage_quota_exceeded"))
        assertEquals(ApiFailure.Conflict, failureOf(HttpStatusCode.Conflict, "conflict"))
    }

    @Test
    fun `the status decides together with the code`() {
        // Same code, another status: it is not what the contract documents, so it stays a plain failure.
        assertEquals(
            503,
            (
                failureOf(
                    HttpStatusCode.ServiceUnavailable,
                    "storage_quota_exceeded",
                ) as ApiFailure.Unexpected
            ).httpStatus,
        )
    }
}
