package com.teachme.core.network

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SseParserTest {
    private fun frames(vararg lines: String): List<SseFrame> {
        val parser = SseParser()
        return lines.mapNotNull { parser.feed(it) }
    }

    @Test
    fun `dispatches a frame on the blank line`() {
        val result = frames("id: 42", "event: message_delta", "data: {\"a\":1}", "")
        assertEquals(listOf(SseFrame("42", "message_delta", "{\"a\":1}")), result)
    }

    @Test
    fun `comments and heartbeats are ignored`() {
        assertEquals(emptyList<SseFrame>(), frames(": ping", ""))
        assertEquals(1, frames(": ping", "data: x", ": ping", "").size)
    }

    @Test
    fun `multiple data lines are joined with a newline`() {
        assertEquals("a\nb", frames("data: a", "data: b", "").single().data)
    }

    @Test
    fun `only one leading space of the value is removed`() {
        assertEquals("  x", frames("data:   x", "").single().data)
        assertEquals("x", frames("data:x", "").single().data)
    }

    @Test
    fun `id persists across frames and event resets`() {
        val result = frames("id: 1", "event: a", "data: 1", "", "data: 2", "")
        assertEquals("1", result[1].id)
        assertNull(result[1].event)
    }

    @Test
    fun `a frame without data is not dispatched`() {
        assertEquals(emptyList<SseFrame>(), frames("id: 5", "event: x", ""))
    }

    @Test
    fun `unknown fields and retry are ignored`() {
        assertEquals(1, frames("retry: 3000", "foo: bar", "data: x", "").size)
    }
}
