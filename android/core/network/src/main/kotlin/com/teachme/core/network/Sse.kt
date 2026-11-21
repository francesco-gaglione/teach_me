package com.teachme.core.network

/** One dispatched Server-Sent Events frame. [id] is the last `id` field seen (it persists across frames). */
internal data class SseFrame(
    val id: String?,
    val event: String?,
    val data: String,
)

/**
 * Incremental parser of the `text/event-stream` format, fed one line at a time (without the line
 * terminator). Comment lines (`: ping`) and unknown fields are ignored; a blank line dispatches the
 * frame if it has data. Tolerant by design: nothing here throws.
 */
internal class SseParser {
    private var id: String? = null
    private var event: String? = null
    private val data = StringBuilder()
    private var hasData = false

    /** Returns the frame completed by [line], or null. */
    fun feed(line: String): SseFrame? {
        if (line.isEmpty()) return dispatch()
        if (line.startsWith(':')) return null
        val colon = line.indexOf(':')
        val field = if (colon < 0) line else line.substring(0, colon)
        var value = if (colon < 0) "" else line.substring(colon + 1)
        if (value.startsWith(' ')) value = value.substring(1)
        when (field) {
            "id" -> if ('\u0000' !in value) id = value
            "event" -> event = value
            "data" -> {
                if (hasData) data.append('\n')
                data.append(value)
                hasData = true
            }
        }
        return null
    }

    private fun dispatch(): SseFrame? {
        val frame = if (hasData) SseFrame(id, event?.takeIf { it.isNotEmpty() }, data.toString()) else null
        event = null
        data.setLength(0)
        hasData = false
        return frame
    }
}
