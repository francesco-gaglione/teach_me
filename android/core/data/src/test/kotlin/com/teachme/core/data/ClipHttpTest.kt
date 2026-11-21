package com.teachme.core.data

import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Request
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

internal class ClipHttpTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer().also { it.start() }
    }

    @After
    fun tearDown() = server.close()

    private fun base() = server.url("/").toString().trimEnd('/')

    private fun get(
        client: okhttp3.OkHttpClient,
        path: String,
        token: String = "tm_token",
    ): okhttp3.Response {
        val url = checkNotNull(ClipHttp.resolve(base(), path))
        val request = Request.Builder().url(url)
        ClipHttp.authorization(token).forEach { (k, v) -> request.header(k, v) }
        return client.newCall(request.build()).execute()
    }

    private fun clip(body: String = "audio") =
        MockResponse
            .Builder()
            .body(body)
            .addHeader("Cache-Control", "private, max-age=31536000, immutable")
            .build()

    @Test
    fun `the clip is requested at base plus api v1 plus the path with the bearer token`() {
        server.enqueue(clip())
        get(ClipHttp.client(tmp.newFolder()), "/audio/clips/abc").use { assertEquals("audio", it.body.string()) }
        val request = server.takeRequest()
        assertEquals("/api/v1/audio/clips/abc", request.target)
        assertEquals("Bearer tm_token", request.headers["Authorization"])
    }

    @Test
    fun `a path without the leading slash resolves the same`() {
        assertEquals(ClipHttp.resolve(base(), "/audio/clips/a"), ClipHttp.resolve(base(), "audio/clips/a"))
        assertEquals("/api/v1/audio/clips/a", ClipHttp.resolve("http://h:8080/", "audio/clips/a")?.encodedPath)
    }

    @Test
    fun `urls to another host or outside api v1 are refused`() {
        assertNull(ClipHttp.resolve("http://teach.test", "http://evil.test/x"))
        assertEquals("teach.test", ClipHttp.resolve("http://teach.test", "//evil.test/x")?.host)
        assertNull(ClipHttp.resolve("http://teach.test", "../../secret"))
        assertNull(ClipHttp.resolve("not a url", "/audio/clips/a"))
    }

    @Test
    fun `a redirect to another host is not followed and the token goes nowhere else`() {
        val other = MockWebServer().also { it.start() }
        try {
            server.enqueue(
                MockResponse
                    .Builder()
                    .code(302)
                    .addHeader("Location", other.url("/steal").toString())
                    .build(),
            )
            get(ClipHttp.client(tmp.newFolder()), "/audio/clips/a").use { assertEquals(302, it.code) }
            assertEquals(1, server.requestCount)
            assertEquals(0, other.requestCount)
        } finally {
            other.close()
        }
    }

    @Test
    fun `the second play is served from the disk cache`() {
        server.enqueue(clip("clip-bytes"))
        val client = ClipHttp.client(tmp.newFolder())
        get(client, "/audio/clips/a").use { assertEquals("clip-bytes", it.body.string()) }
        get(client, "/audio/clips/a").use { assertEquals("clip-bytes", it.body.string()) }
        assertEquals(1, server.requestCount)
    }

    @Test
    fun `a stale ended event of the previous clip is not current`() {
        val generation = PlaybackGeneration()
        val a = generation.next()
        assertTrue(generation.isCurrent(a))
        val b = generation.next()
        assertFalse(generation.isCurrent(a))
        assertTrue(generation.isCurrent(b))
        generation.invalidate()
        assertFalse(generation.isCurrent(b))
        assertFalse(generation.isCurrent(null))
    }
}
