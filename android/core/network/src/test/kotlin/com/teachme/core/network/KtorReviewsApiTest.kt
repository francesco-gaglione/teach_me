package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.DueTopic
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewRating
import com.teachme.core.model.ReviewState
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.SuspensionReason
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class KtorReviewsApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        KtorReviewsApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0")))

    private fun cardJson(
        kind: String = "record",
        status: String = "active",
        state: String = "review",
        preview: String = """{"again":600,"hard":172800,"good":432000,"easy":950400}""",
        reviewCount: Int = 3,
        suspension: String = "null",
        suspendedAt: String = "null",
        toConsolidate: String = "",
    ) = """{"id":"c1","topic_id":"t1","topic_title":"Rust","lesson_id":"l1","lesson_number":2,
        |"lesson_title":"Result e operatore ?","kind":"$kind","front":"Davanti","back":"Dietro","status":"$status",
        |"suspension":$suspension,"suspended_at":$suspendedAt,$toConsolidate"state":"$state","due":"2026-11-12T09:00:00.000Z","last_reviewed_at":null,"review_count":$reviewCount,
        |"intervals_preview":$preview,"future_field":1}
        """.trimMargin().replace("\n", "")

    @Test
    fun `due reads the page, the totals and tolerates unknown enum values`() =
        runBlocking {
            var url = ""
            val body =
                """{"items":[${cardJson()},${cardJson(kind = "weird", state = "odd", status = "other")}],
                |"next_cursor":null,"due_count":12,"estimated_minutes":3,"next_due_at":"2026-11-10T09:00:00.000Z"}
                """.trimMargin().replace("\n", "")
            val due =
                (
                    api {
                        url = it.url.toString()
                        respond(body, HttpStatusCode.OK, json)
                    }.due(20) as ApiResult.Success
                ).value
            assertTrue(url.endsWith("/api/v1/reviews/due?limit=20"))
            assertEquals(12, due.dueCount)
            assertEquals(3, due.estimatedMinutes)
            assertEquals(Instant.parse("2026-11-10T09:00:00Z"), due.nextDueAt)
            val first = due.items[0]
            assertEquals(ReviewCardKind.Record, first.kind)
            assertEquals(ReviewState.Review, first.state)
            assertEquals(3, first.reviewCount)
            assertEquals(172_800L, first.intervals?.forRating(ReviewRating.Hard))
            assertNull(first.lastReviewedAt)
            // Unknown values: generic card, active, read as `review`.
            assertEquals(ReviewCardKind.Unknown, due.items[1].kind)
            assertEquals(ReviewCardStatus.Active, due.items[1].status)
            assertEquals(ReviewState.Review, due.items[1].state)
        }

    @Test
    fun `due with a topic sends topic_id, reads topics and maps 404 to NotFound`() =
        runBlocking {
            var url = ""
            val body =
                """{"items":[],"due_count":0,"estimated_minutes":0,"next_due_at":null,
                |"topics":[{"topic_id":"t1","topic_title":"Rust","due_count":8,"x":1}]}
                """.trimMargin().replace("\n", "")
            val due =
                (
                    api {
                        url = it.url.toString()
                        respond(body, HttpStatusCode.OK, json)
                    }.due(20, "t1") as ApiResult.Success
                ).value
            assertTrue(url.endsWith("/api/v1/reviews/due?limit=20&topic_id=t1"))
            assertEquals(listOf(DueTopic("t1", "Rust", 8)), due.topics)
            assertEquals(
                ApiFailure.NotFound,
                (
                    api { respond("""{"error":{"code":"not_found","message":"m"}}""", HttpStatusCode.NotFound, json) }
                        .due(20, "t1") as ApiResult.Failure
                ).failure,
            )
        }

    @Test
    fun `due without next_due_at and a suspended card without previews`() =
        runBlocking {
            val body =
                """{"items":[${cardJson(status = "suspended", preview = "null")}],"due_count":0,"estimated_minutes":0,
                |"next_due_at":null}
                """.trimMargin().replace("\n", "")
            val due = (api { respond(body, HttpStatusCode.OK, json) }.due(1) as ApiResult.Success).value
            assertNull(due.nextDueAt)
            assertEquals(ReviewCardStatus.Suspended, due.items[0].status)
            assertNull(due.items[0].intervals)
        }

    @Test
    fun `rate posts the grade with the card version and the app's clock in UTC milliseconds`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            var sent = ""
            val result =
                api {
                    method = it.method
                    path = it.url.encodedPath
                    sent = it.body.toByteArray().decodeToString()
                    respond(cardJson(reviewCount = 4), HttpStatusCode.OK, json)
                }.rate(QueuedReview("c1", "good", Instant.parse("2026-11-10T09:00:05Z").toEpochMilli(), 3))
            assertEquals(HttpMethod.Post, method)
            assertEquals("/api/v1/reviews/c1", path)
            assertTrue(sent, sent.contains(""""rating":"good""""))
            assertTrue(sent, sent.contains(""""reviewed_at":"2026-11-10T09:00:05.000Z""""))
            assertTrue(sent, sent.contains(""""review_count":3"""))
            assertEquals(4, (result as ApiResult.Success).value.reviewCount)
        }

    @Test
    fun `rate maps the documented errors`() =
        runBlocking {
            fun error(
                status: HttpStatusCode,
                code: String,
            ) = api { respond("""{"error":{"code":"$code","message":"m"}}""", status, json) }
            val review = QueuedReview("c1", "good", 1L, 3)
            assertEquals(
                ApiFailure.Conflict,
                (error(HttpStatusCode.Conflict, "conflict").rate(review) as ApiResult.Failure).failure,
            )
            assertEquals(
                ApiFailure.ReviewConflict.AlreadyReviewed,
                (error(HttpStatusCode.Conflict, "already_reviewed").rate(review) as ApiResult.Failure).failure,
            )
            assertEquals(
                ApiFailure.ReviewConflict.CardSuspended,
                (error(HttpStatusCode.Conflict, "card_suspended").rate(review) as ApiResult.Failure).failure,
            )
            assertEquals(
                ApiFailure.NotFound,
                (error(HttpStatusCode.NotFound, "not_found").rate(review) as ApiResult.Failure).failure,
            )
            assertTrue(
                (
                    error(
                        HttpStatusCode.UnprocessableEntity,
                        "validation_failed",
                    ).rate(review) as ApiResult.Failure
                ).failure is ApiFailure.Validation,
            )
            assertEquals(
                ApiFailure.Unauthorized,
                (error(HttpStatusCode.Unauthorized, "unauthorized").rate(review) as ApiResult.Failure).failure,
            )
        }

    @Test
    fun `undo and suspend post to their paths`() =
        runBlocking {
            val paths = mutableListOf<String>()
            val api =
                api {
                    paths += "${it.method.value} ${it.url.encodedPath}"
                    respond(cardJson(status = "suspended", preview = "null"), HttpStatusCode.OK, json)
                }
            api.undo("c9")
            val suspended = (api.suspend("c9") as ApiResult.Success).value
            assertEquals(listOf("POST /api/v1/reviews/c9/undo", "POST /api/v1/reviews/c9/suspend"), paths)
            assertEquals(ReviewCardStatus.Suspended, suspended.status)
            assertNull(suspended.intervals)
        }

    @Test
    fun `a card without review_count is invalid`() =
        runBlocking {
            val body = """{"items":[{"id":"c1","front":"f","back":"b"}],"due_count":1,"estimated_minutes":1}"""
            val result = api { respond(body, HttpStatusCode.OK, json) }.due(1)
            assertTrue((result as ApiResult.Failure).failure is ApiFailure.Unexpected)
        }

    private fun conflict(code: String) =
        api { respond("""{"error":{"code":"$code","message":"m"}}""", HttpStatusCode.Conflict, json) }

    @Test
    fun `undo and unsuspend map their 409 codes, an unknown code stays the generic conflict`() =
        runBlocking {
            suspend fun failure(
                code: String,
                call: suspend (KtorReviewsApi) -> ApiResult<*>,
            ) = (call(conflict(code)) as ApiResult.Failure).failure
            assertEquals(ApiFailure.ReviewConflict.NothingToUndo, failure("nothing_to_undo") { it.undo("c1") })
            assertEquals(ApiFailure.ReviewConflict.UndoExpired, failure("undo_expired") { it.undo("c1") })
            assertEquals(ApiFailure.ReviewConflict.CardSuperseded, failure("card_superseded") { it.unsuspend("c1") })
            assertEquals(ApiFailure.Conflict, failure("something_new") { it.undo("c1") })
            assertEquals(ApiFailure.Conflict, failure("conflict") { it.unsuspend("c1") })
        }

    @Test
    fun `suspension and suspended_at are read, an unknown reason is superseded`() =
        runBlocking {
            val body =
                """{"items":[${cardJson(
                    status = "suspended",
                    preview = "null",
                    suspension = "\"user\"",
                    suspendedAt = "\"2026-11-15T08:00:00.000Z\"",
                )},${cardJson(status = "suspended", preview = "null", suspension = "\"new_reason\"")},${cardJson()}],
                |"next_cursor":null,"due_count":0,"estimated_minutes":0,"next_due_at":null}
                """.trimMargin().replace("\n", "")
            val items = (api { respond(body, HttpStatusCode.OK, json) }.due(5) as ApiResult.Success).value.items
            assertEquals(SuspensionReason.User, items[0].suspension)
            assertEquals(Instant.parse("2026-11-15T08:00:00Z"), items[0].suspendedAt)
            assertEquals(SuspensionReason.Superseded, items[1].suspension)
            assertNull(items[1].suspendedAt)
            assertNull(items[2].suspension)
        }

    @Test
    fun `to_consolidate is read and defaults to false when absent`() =
        runBlocking {
            val body =
                """{"items":[${cardJson(toConsolidate = "\"to_consolidate\":true,")},${cardJson()}],
                |"next_cursor":null,"due_count":0,"estimated_minutes":0,"next_due_at":null}
                """.trimMargin().replace("\n", "")
            val items = (api { respond(body, HttpStatusCode.OK, json) }.due(5) as ApiResult.Success).value.items
            assertTrue(items[0].toConsolidate)
            assertFalse(items[1].toConsolidate)
        }

    @Test
    fun `unsuspend posts to its path and returns the active card`() =
        runBlocking {
            var call = ""
            val card =
                (
                    api {
                        call = "${it.method.value} ${it.url.encodedPath}"
                        respond(cardJson(), HttpStatusCode.OK, json)
                    }.unsuspend("c9") as ApiResult.Success
                ).value
            assertEquals("POST /api/v1/reviews/c9/unsuspend", call)
            assertEquals(ReviewCardStatus.Active, card.status)
            assertNull(card.suspension)
        }

    @Test
    fun `suspended reads the page with its cursor and the total`() =
        runBlocking {
            val urls = mutableListOf<String>()
            val suspendedCard =
                cardJson(
                    status = "suspended",
                    preview = "null",
                    suspension = "\"user\"",
                    suspendedAt = "\"2026-11-15T08:00:00.000Z\"",
                )
            val api =
                api {
                    urls += it.url.toString()
                    val body = """{"items":[$suspendedCard],"next_cursor":"abc","suspended_count":7}"""
                    respond(body, HttpStatusCode.OK, json)
                }
            val first = (api.suspended(50, null) as ApiResult.Success).value
            val second = (api.suspended(50, "abc") as ApiResult.Success).value
            assertTrue(urls[0].endsWith("/api/v1/reviews/suspended?limit=50"))
            assertTrue(urls[1].endsWith("/api/v1/reviews/suspended?limit=50&cursor=abc"))
            assertEquals(7, first.suspendedCount)
            assertEquals("abc", first.nextCursor)
            assertEquals(1, second.items.size)
            assertEquals(SuspensionReason.User, first.items[0].suspension)
        }

    @Test
    fun `suspended without next_cursor is the last page`() =
        runBlocking {
            val page =
                (
                    api { respond("""{"items":[],"next_cursor":null,"suspended_count":0}""", HttpStatusCode.OK, json) }
                        .suspended(1, null) as ApiResult.Success
                ).value
            assertNull(page.nextCursor)
            assertEquals(0, page.suspendedCount)
        }
}
