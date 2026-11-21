package com.teachme.core.network

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AttemptOutcome
import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.DocumentSource
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.QualityWarning
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.RecordKind
import com.teachme.core.model.RecordStatus
import com.teachme.core.model.SettingsRepository
import com.teachme.core.model.StrokeRecord
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
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class KtorLessonsApiTest {
    private val json = headersOf(HttpHeaders.ContentType, "application/json")
    private val topicId = "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e70"
    private val lessonId = "01933b4e-9a1b-7c3d-9e1f-2a3b4c5d6e71"

    private class FakeSettings : SettingsRepository {
        private val state = MutableStateFlow(BackendSettings("http://10.0.2.2:8080", "secret-token"))
        override val settings: Flow<BackendSettings> = state

        override suspend fun current() = state.value

        override suspend fun update(settings: BackendSettings) {
            state.value = settings
        }
    }

    private fun api(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) =
        KtorLessonsApi(createBackendHttpClient(MockEngine(handler), FakeSettings(), AppVersion("0.1.0")))

    private fun lessonJson(
        status: String = "in_progress",
        number: Int = 3,
    ) = """{"id":"$lessonId","topic_id":"$topicId","number":$number,"title":"Lifetimes in pratica",
        |"objective":"Saper dire quale riferimento vive più a lungo.","estimated_minutes":8,"status":"$status",
        |"catalog_version":1,"mission_version":3,"created_at":"2026-11-09T15:02:11.420Z","completed_at":null,
        |"future_field":true}
        """.trimMargin().replace("\n", "")

    private fun jobJson(
        kind: String,
        status: String = "queued",
        lesson: String? = null,
        result: String = "null",
        progress: String = "null",
    ) = """{"id":"job1","topic_id":"$topicId","lesson_id":${lesson?.let { "\"$it\"" }},"kind":"$kind",
        |"status":"$status","progress":$progress,"error":null,"result":$result,"attempt":1,
        |"created_at":"2026-11-09T15:00:00.000Z","started_at":null,"finished_at":null}
        """.trimMargin().replace("\n", "")

    @Test
    fun `get lesson keeps the document raw, restores attempts and tolerates unknown values`() =
        runBlocking {
            var path = ""
            val body =
                """{"lesson":${lessonJson("surprise")},
                |"document":{"title":"T","blocks":[{"id":"a","type":"paragraph","text":"x"}]},
                |"attempts":[
                |{"id":"at1","block_id":"q1","block_type":"quiz_single","attempt_number":1,"answer":{"option":"b"},
                |"outcome":"correct","grade":null,"answered_at":"2026-11-09T15:08:40.000Z","created_at":"2026-11-09T15:08:41.000Z"},
                |{"id":"at2","block_id":"q2","block_type":"open_answer","attempt_number":1,"answer":{"text":"Perché"},
                |"outcome":"partial","grade":{"verdict":"partial","feedback":"Manca il *perché*.","misconception":null},
                |"answered_at":"2026-11-09T15:10:02.000Z"},
                |{"id":"at3","block_id":"q3","block_type":"open_answer","attempt_number":1,"answer":{"text":"x"},
                |"outcome":"pending","grade":null,"answered_at":"2026-11-09T15:11:02.000Z"},
                |{"id":"at4","block_id":"q4","block_type":"future","attempt_number":1,"answer":"weird","outcome":"maybe"}],
                |"completion":null}
                """.trimMargin().replace("\n", "")
            val view =
                (
                    api {
                        path = it.url.encodedPath
                        respond(body, HttpStatusCode.OK, json)
                    }.getLesson(lessonId) as ApiResult.Success
                ).value
            assertEquals("/api/v1/lessons/$lessonId", path)
            assertEquals(LessonStatus.NotStarted, view.lesson.status)
            assertEquals(3, view.lesson.number)
            assertTrue(view.documentJson.contains("\"blocks\""))
            assertEquals(4, view.attempts.size)
            assertEquals(AttemptOutcome.Correct, view.attempts[0].outcome)
            assertEquals(buildJsonObject { put("option", "b") }, view.attempts[0].answer)
            assertEquals("Manca il *perché*.", view.attempts[1].grade?.feedback)
            assertEquals(AttemptOutcome.Pending, view.attempts[2].outcome)
            assertNull(view.attempts[2].grade)
            assertEquals(AttemptOutcome.Unknown, view.attempts[3].outcome)
            assertTrue(view.attempts[3].answer.isEmpty())
            assertNull(view.completion)
        }

    @Test
    fun `get lesson maps the completion with its job, records and terms`() =
        runBlocking {
            val body =
                """{"lesson":${lessonJson("completed")},"document":{"blocks":[]},"attempts":[],
                |"completion":{"job":${jobJson(
                    "lesson_completion",
                    "succeeded",
                    lessonId,
                    """{"record_count":1,"term_count":1,"review_card_count":3}""",
                )},
                |"records":[{"id":"r1","number":4,"kind":"misconception_corrected","title":"T","body":"B","evidence":"E",
                |"status":"active","superseded_by":null,"lesson_id":"$lessonId","created_at":"2026-11-09T15:14:40.002Z"},
                |{"id":"r2","number":3,"kind":"future_kind","title":"T","body":"B","evidence":"E","status":"superseded",
                |"superseded_by":"r1","lesson_id":"$lessonId"}],
                |"terms":[{"id":"t1","key":"ownership","term":"ownership","definition":"D","avoid":["proprietà"],
                |"lesson_id":"$lessonId","created_at":"2026-11-09T15:14:40.002Z","updated_at":"2026-11-09T16:00:00.000Z"}],
                |"review_card_count":3}}
                """.trimMargin().replace("\n", "")
            val view = (api { respond(body, HttpStatusCode.OK, json) }.getLesson(lessonId) as ApiResult.Success).value
            val completion = view.completion!!
            assertEquals(JobStatus.Succeeded, completion.job?.status)
            assertEquals(lessonId, completion.job?.lessonId)
            assertEquals(3, completion.job?.result?.reviewCardCount)
            assertEquals(RecordKind.MisconceptionCorrected, completion.records[0].kind)
            assertEquals(RecordKind.Unknown, completion.records[1].kind)
            assertEquals(RecordStatus.Superseded, completion.records[1].status)
            assertEquals("r1", completion.records[1].supersededBy)
            assertEquals(listOf("proprietà"), completion.terms[0].avoid)
            assertTrue(completion.terms[0].revised)
            assertEquals(3, completion.reviewCardCount)
        }

    @Test
    fun `list lessons pages with the cursor and the limit`() =
        runBlocking {
            var query = ""
            val page =
                (
                    api {
                        query = it.url.encodedQuery
                        respond("""{"items":[${lessonJson()}],"next_cursor":"c2"}""", HttpStatusCode.OK, json)
                    }.listLessons(topicId, "c1", 20) as ApiResult.Success
                ).value
            assertEquals("cursor=c1&limit=20", query)
            assertEquals("c2", page.nextCursor)
            assertEquals(lessonId, page.items.single().id)
        }

    @Test
    fun `start generation posts with an empty body and maps 409`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            val job =
                (
                    api {
                        method = it.method
                        path = it.url.encodedPath
                        respond(
                            jobJson(
                                "lesson_generation",
                                progress = """{"step":"planning","message":null,"done":null,"total":null}""",
                            ),
                            HttpStatusCode.Accepted,
                            json,
                        )
                    }.startGeneration(topicId) as ApiResult.Success
                ).value
            assertEquals(HttpMethod.Post, method)
            assertEquals("/api/v1/topics/$topicId/lessons", path)
            assertTrue(job.isLessonGeneration)
            assertEquals("planning", job.progress?.step)
            val conflict =
                api { respond("""{"error":{"code":"conflict","message":"busy"}}""", HttpStatusCode.Conflict, json) }
            assertEquals(ApiResult.Failure(ApiFailure.Conflict), conflict.startGeneration(topicId))
        }

    @Test
    fun `submit attempts sends the contract body and returns the saved attempts`() =
        runBlocking {
            var body = ""
            val attempts =
                listOf(
                    QueuedAttempt(
                        lessonId,
                        "q1",
                        "quiz_single",
                        1,
                        buildJsonObject { put("option", "b") },
                        "correct",
                        1_762_700_920_000L,
                    ),
                    QueuedAttempt(
                        lessonId,
                        "q2",
                        "open_answer",
                        1,
                        buildJsonObject { put("text", "Perché") },
                        null,
                        1_762_701_002_000L,
                    ),
                )
            val saved =
                (
                    api {
                        body = String(it.body.toByteArray())
                        respond(
                            """{"attempts":[
                            |{"id":"a1","block_id":"q1","block_type":"quiz_single","attempt_number":1,"answer":{"option":"b"},"outcome":"correct"},
                            |{"id":"a2","block_id":"q2","block_type":"open_answer","attempt_number":1,"answer":{"text":"Perché"},"outcome":"pending"}]}
                            """.trimMargin(),
                            HttpStatusCode.OK,
                            json,
                        )
                    }.submitAttempts(lessonId, attempts, timeoutMs = 25_000) as ApiResult.Success
                ).value
            assertTrue(
                body.contains(
                    """"block_id":"q1","attempt_number":1,"answer":{"option":"b"},"outcome":"correct","answered_at":"2025-11-09T""",
                ),
            )
            // A flashcard or an open answer has no `outcome` key at all.
            assertTrue(
                body.contains(""""block_id":"q2","attempt_number":1,"answer":{"text":"Perché"},"answered_at":""""),
            )
            assertEquals(listOf(AttemptOutcome.Correct, AttemptOutcome.Pending), saved.map { it.outcome })
        }

    @Test
    fun `submit attempts maps 409, 404 and 422 with the index of the bad attempt`() =
        runBlocking {
            val one =
                listOf(
                    QueuedAttempt(
                        lessonId,
                        "q1",
                        "quiz_single",
                        1,
                        buildJsonObject { put("option", "b") },
                        "correct",
                        0,
                    ),
                )
            val conflict =
                api { respond("""{"error":{"code":"conflict","message":"closed"}}""", HttpStatusCode.Conflict, json) }
            assertEquals(ApiResult.Failure(ApiFailure.Conflict), conflict.submitAttempts(lessonId, one))
            val gone =
                api { respond("""{"error":{"code":"not_found","message":"x"}}""", HttpStatusCode.NotFound, json) }
            assertEquals(ApiResult.Failure(ApiFailure.NotFound), gone.submitAttempts(lessonId, one))
            val invalid =
                api {
                    respond(
                        """{"error":{"code":"validation_failed","message":"m",
                        |"details":[{"field":"attempts[1].answer","issue":"invalid"}]}}
                        """.trimMargin(),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }
            val failure = (invalid.submitAttempts(lessonId, one) as ApiResult.Failure).failure as ApiFailure.Validation
            assertTrue(failure.hasIssue("attempts[1].answer", "invalid"))
        }

    @Test
    fun `get lesson reads the highlights and an absent field means none`() =
        runBlocking {
            val withField =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null,
                        |"highlights":[{"block_id":"p3","start":12,"end":47,"color":"yellow"},
                        |{"block_id":"p4","start":0,"end":5,"color":"purple"}]}
                        """.trimMargin(),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            val view = (withField.getLesson(lessonId) as ApiResult.Success).value
            // An unknown colour stays a string here: the lesson feature reads it as yellow.
            assertEquals(
                listOf(HighlightRecord("p3", 12, 47, "yellow"), HighlightRecord("p4", 0, 5, "purple")),
                view.highlights,
            )
            val oldBackend =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(
                emptyList<HighlightRecord>(),
                (oldBackend.getLesson(lessonId) as ApiResult.Success).value.highlights,
            )
        }

    @Test
    fun `quality warnings are read, default to empty, keep a null block id and an unknown code`() =
        runBlocking {
            val present =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null,
                        |"quality_warnings":[{"code":"too_many_callouts","block_id":"c1","message":"Troppe note."},
                        |{"code":"consecutive_code_blocks","block_id":null,"message":"Due codici."},
                        |{"code":"from_the_future","block_id":"p9","message":"Nuovo avviso."},
                        |{"code":"too_many_callouts","block_id":"c2","message":" "}]}
                        """.trimMargin(),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(
                listOf(
                    QualityWarning("too_many_callouts", "c1", "Troppe note."),
                    QualityWarning("consecutive_code_blocks", null, "Due codici."),
                    QualityWarning("from_the_future", "p9", "Nuovo avviso."),
                ),
                (present.getLesson(lessonId) as ApiResult.Success).value.qualityWarnings,
            )
            val absent =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(
                emptyList<QualityWarning>(),
                (absent.getLesson(lessonId) as ApiResult.Success).value.qualityWarnings,
            )
        }

    @Test
    fun `put highlights sends the whole set and maps 404 and the 422 with its index`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            var body = ""
            val set = listOf(HighlightRecord("p3", 12, 47, "yellow"))
            val ok =
                api {
                    method = it.method
                    path = it.url.encodedPath
                    body = String(it.body.toByteArray())
                    respond(
                        """{"highlights":[{"block_id":"p3","start":12,"end":47,"color":"yellow"}]}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(ApiResult.Success(set), ok.putHighlights(lessonId, set))
            assertEquals(HttpMethod.Put, method)
            assertEquals("/api/v1/lessons/$lessonId/highlights", path)
            assertEquals("""{"highlights":[{"block_id":"p3","start":12,"end":47,"color":"yellow"}]}""", body)
            api {
                body = String(it.body.toByteArray())
                respond("""{"highlights":[]}""", HttpStatusCode.OK, json)
            }.putHighlights(lessonId, emptyList())
            assertEquals("""{"highlights":[]}""", body)
            val gone =
                api { respond("""{"error":{"code":"not_found","message":"x"}}""", HttpStatusCode.NotFound, json) }
            assertEquals(ApiResult.Failure(ApiFailure.NotFound), gone.putHighlights(lessonId, set))
            val invalid =
                api {
                    respond(
                        """{"error":{"code":"validation_failed","message":"m",
                        |"details":[{"field":"highlights[0].start","issue":"overlap"}]}}
                        """.trimMargin(),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }
            val failure = (invalid.putHighlights(lessonId, set) as ApiResult.Failure).failure as ApiFailure.Validation
            assertTrue(failure.hasIssue("highlights[0].start", "overlap"))
        }

    @Test
    fun `get lesson reads the strokes in order and an absent field means none`() =
        runBlocking {
            val withField =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null,
                        |"strokes":[{"id":"b","block_id":"p3","color":"red","width":"thick","points":[1,2,3,4]},
                        |{"id":"a","block_id":"p4","color":"neon","width":"thin","points":[5,6]}]}
                        """.trimMargin(),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            val view = (withField.getLesson(lessonId) as ApiResult.Success).value
            // The order is the drawing order; an unknown colour stays a string here.
            assertEquals(
                listOf(
                    StrokeRecord("b", "p3", "red", "thick", listOf(1, 2, 3, 4)),
                    StrokeRecord("a", "p4", "neon", "thin", listOf(5, 6)),
                ),
                view.strokes,
            )
            val oldBackend =
                api {
                    respond(
                        """{"lesson":${lessonJson()},"document":{"title":"T","blocks":[]},"attempts":[],"completion":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(
                emptyList<StrokeRecord>(),
                (oldBackend.getLesson(lessonId) as ApiResult.Success).value.strokes,
            )
        }

    @Test
    fun `put strokes sends the whole set and maps 404, 413 and the 422 with its index`() =
        runBlocking {
            var method: HttpMethod? = null
            var path = ""
            var body = ""
            val set = listOf(StrokeRecord("a", "p3", "ink", "medium", listOf(1204, 380)))
            val ok =
                api {
                    method = it.method
                    path = it.url.encodedPath
                    body = String(it.body.toByteArray())
                    respond(
                        """{"strokes":[{"id":"a","block_id":"p3","color":"ink","width":"medium","points":[1204,380]}]}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(ApiResult.Success(set), ok.putStrokes(lessonId, set))
            assertEquals(HttpMethod.Put, method)
            assertEquals("/api/v1/lessons/$lessonId/strokes", path)
            assertEquals(
                """{"strokes":[{"id":"a","block_id":"p3","color":"ink","width":"medium","points":[1204,380]}]}""",
                body,
            )
            api {
                body = String(it.body.toByteArray())
                respond("""{"strokes":[]}""", HttpStatusCode.OK, json)
            }.putStrokes(lessonId, emptyList())
            assertEquals("""{"strokes":[]}""", body)
            val gone =
                api { respond("""{"error":{"code":"not_found","message":"x"}}""", HttpStatusCode.NotFound, json) }
            assertEquals(ApiResult.Failure(ApiFailure.NotFound), gone.putStrokes(lessonId, set))
            val tooBig =
                api {
                    respond(
                        """{"error":{"code":"payload_too_large","message":"x"}}""",
                        HttpStatusCode.PayloadTooLarge,
                        json,
                    )
                }
            val big = (tooBig.putStrokes(lessonId, set) as ApiResult.Failure).failure as ApiFailure.Unexpected
            assertEquals(413, big.httpStatus)
            val invalid =
                api {
                    respond(
                        """{"error":{"code":"validation_failed","message":"m",
                        |"details":[{"field":"strokes[0].points","issue":"out_of_range"}]}}
                        """.trimMargin(),
                        HttpStatusCode.UnprocessableEntity,
                        json,
                    )
                }
            val failure = (invalid.putStrokes(lessonId, set) as ApiResult.Failure).failure as ApiFailure.Validation
            assertTrue(failure.hasIssue("strokes[0].points", "out_of_range"))
        }

    @Test
    fun `complete posts to the complete path`() =
        runBlocking {
            var path = ""
            val job =
                (
                    api {
                        path = it.url.encodedPath
                        respond(jobJson("lesson_completion", lesson = lessonId), HttpStatusCode.Accepted, json)
                    }.completeLesson(lessonId) as ApiResult.Success
                ).value
            assertEquals("/api/v1/lessons/$lessonId/complete", path)
            assertTrue(job.isLessonCompletion)
            assertEquals(lessonId, job.lessonId)
        }

    @Test
    fun `glossary, records and jobs use their paths and parameters`() =
        runBlocking {
            val seen = mutableListOf<String>()
            val api =
                api {
                    seen += it.url.encodedPath + "?" + it.url.encodedQuery
                    when {
                        it.url.encodedPath.endsWith("/glossary") ->
                            respond(
                                """{"items":[{"id":"t1","key":"ownership","term":"ownership","definition":"D"}],"next_cursor":null}""",
                                HttpStatusCode.OK,
                                json,
                            )
                        it.url.encodedPath.endsWith(
                            "/learning-records",
                        ) -> respond("""{"items":[],"next_cursor":null}""", HttpStatusCode.OK, json)
                        else ->
                            respond(
                                """{"items":[${jobJson(
                                    "lesson_generation",
                                    "succeeded",
                                    result = """{"lesson_id":"$lessonId","lesson_number":3}""",
                                )}],"next_cursor":null}""",
                                HttpStatusCode.OK,
                                json,
                            )
                    }
                }
            val terms = (api.listGlossary(topicId, null, 200) as ApiResult.Success).value.items
            assertEquals("ownership", terms.single().term)
            assertEquals(emptyList<String>(), terms.single().avoid)
            api.listRecords(topicId, null, 200)
            val jobs = (api.listJobs(topicId, null, 50) as ApiResult.Success).value.items
            assertEquals(lessonId, jobs.single().result?.lessonId)
            assertEquals(3, jobs.single().result?.lessonNumber)
            assertEquals("/api/v1/topics/$topicId/glossary?limit=200", seen[0])
            assertEquals("/api/v1/topics/$topicId/learning-records?status=all&limit=200", seen[1])
            assertEquals("/api/v1/topics/$topicId/jobs?limit=50", seen[2])
        }

    @Test
    fun `job_updated events carry the lesson id and the repair progress`() {
        val frame =
            """{"type":"job_updated","payload":{"job":${jobJson(
                "lesson_generation",
                "running",
                progress = """{"step":"writing","message":"Correggo la lezione","done":2,"total":3}""",
            )}}}"""
        val event = decodeChatEvent("job_updated", frame) as ChatEvent.JobUpdated
        assertEquals(2, event.job.progress?.done)
        assertEquals("Correggo la lezione", event.job.progress?.message)
        assertNotNull(event.job)
        assertNull(event.job.lessonId)
    }

    @Test
    fun `document sources parse when present and default to empty when absent`() =
        runBlocking<Unit> {
            val with =
                api {
                    respond(
                        """{"items":[{"id":"$lessonId","topic_id":"$topicId","number":1,"title":"T","document_sources":
                        |[{"document_id":"d1","title":"Libro","page_start":3,"page_end":9}]}],"next_cursor":null}
                        """.trimMargin().replace("\n", ""),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            val sources =
                (with.listLessons(topicId) as ApiResult.Success)
                    .value.items
                    .single()
                    .documentSources
            assertEquals(listOf(DocumentSource("d1", "Libro", 3, 9)), sources)
            val without =
                api {
                    respond(
                        """{"items":[{"id":"$lessonId","topic_id":"$topicId"}],"next_cursor":null}""",
                        HttpStatusCode.OK,
                        json,
                    )
                }
            assertEquals(
                emptyList<DocumentSource>(),
                (without.listLessons(topicId) as ApiResult.Success)
                    .value.items
                    .single()
                    .documentSources,
            )
        }

    @Test
    fun `in progress lessons are requested with status and limit and parsed`() =
        runBlocking<Unit> {
            var url = ""
            val api =
                api {
                    url = it.url.encodedPath + "?" + it.url.encodedQuery
                    respond(
                        """{"items":[{"id":"$lessonId","topic_id":"$topicId","number":2,"title":"T","estimated_minutes":5,
                        |"status":"in_progress","topic_title":"Rust in pratica","last_activity_at":"2026-11-19T10:00:00.000Z"}],
                        |"next_cursor":null}
                        """.trimMargin()
                            .replace("\n", ""),
                        HttpStatusCode.OK,
                        json,
                    )
                }
            val item = (api.listInProgress(1) as ApiResult.Success).value.items.single()
            assertEquals("/api/v1/lessons?status=in_progress&limit=1", url)
            assertEquals("Rust in pratica", item.topicTitle)
            assertEquals(java.time.Instant.parse("2026-11-19T10:00:00Z"), item.lastActivityAt)
            assertEquals(LessonStatus.InProgress, item.lesson.status)
            assertEquals(emptyList<DocumentSource>(), item.lesson.documentSources)
        }
}
