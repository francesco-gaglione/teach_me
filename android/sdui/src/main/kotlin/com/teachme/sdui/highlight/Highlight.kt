package com.teachme.sdui.highlight

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** The four highlight colours of the contract (`lessons.md` section 10). */
@Serializable(with = HighlightColorSerializer::class)
enum class HighlightColor(
    val wire: String,
) {
    YELLOW("yellow"),
    GREEN("green"),
    BLUE("blue"),
    PINK("pink"),
    ;

    companion object {
        /** An unknown value is yellow, as the contract asks. */
        fun fromWire(value: String?): HighlightColor = entries.firstOrNull { it.wire == value } ?: YELLOW
    }
}

object HighlightColorSerializer : KSerializer<HighlightColor> {
    override val descriptor = PrimitiveSerialDescriptor("HighlightColor", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder) = HighlightColor.fromWire(decoder.decodeString())

    override fun serialize(
        encoder: Encoder,
        value: HighlightColor,
    ) = encoder.encodeString(value.wire)
}

/**
 * A mark on the `text` of a `paragraph` or `callout`: `[start, end)` in UTF-16 units of the markdown source.
 * Identified by (blockId, start, end), it has no id.
 */
@Serializable
data class Highlight(
    @SerialName("block_id") val blockId: String,
    val start: Int,
    val end: Int,
    val color: HighlightColor,
) {
    val interval: Interval get() = Interval(start, end)
}

/**
 * Pure operations on the highlights of a lesson. Each one keeps the invariant of the contract: inside a block,
 * intervals never overlap (they may touch). Other blocks are untouched. A block's entries stay where its first entry
 * was, sorted by `start`; the backend sorts the final set by document order anyway.
 */
object Highlights {
    /**
     * Paints `[start, end)` of [blockId] with [color]. Parts of other highlights under it are replaced (what remains
     * on their sides survives); same colour highlights that touch or overlap it become one. Invalid intervals
     * (`start < 0` or `start >= end`) change nothing. With a [map] of the block, a remainder that would colour no
     * displayed char (only markdown markers) is swallowed by the new highlight instead of being kept.
     */
    fun add(
        all: List<Highlight>,
        blockId: String,
        start: Int,
        end: Int,
        color: HighlightColor,
        map: SourceMap? = null,
    ): List<Highlight> {
        if (start < 0 || start >= end) return all
        val resolved = resolve(ownOf(all, blockId), color, start, end, map)
        val kept = resolved.rest.flatMap { cut(it, resolved.range.start, resolved.range.end) }
        return replaceBlock(all, blockId, kept + Highlight(blockId, resolved.range.start, resolved.range.end, color))
    }

    /**
     * The interval [add] will really paint for `[start, end)` with [map]: merged with same colour neighbours and
     * widened over marker-only remainders. Passing it to [add] (with or without the map) gives the same set, so a
     * host that applies the edit itself ends with no invisible fragment.
     */
    fun resolveAdd(
        all: List<Highlight>,
        blockId: String,
        start: Int,
        end: Int,
        color: HighlightColor,
        map: SourceMap,
    ): Interval = resolve(ownOf(all, blockId), color, start, end, map).range

    /**
     * Erases `[start, end)` of [blockId]: what remains of a highlight on either side survives. With a [map], a
     * remainder that colours no displayed char (only markers) is erased too.
     */
    fun remove(
        all: List<Highlight>,
        blockId: String,
        start: Int,
        end: Int,
        map: SourceMap? = null,
    ): List<Highlight> {
        if (start >= end) return all
        val own = ownOf(all, blockId)
        val range = if (map == null) Interval(start, end) else absorbMarkerOnly(own, start, end, map)
        return replaceBlock(all, blockId, own.flatMap { cut(it, range.start, range.end) })
    }

    /** The interval [remove] will really erase for `[start, end)` with [map] (see [resolveAdd]). */
    fun resolveRemove(
        all: List<Highlight>,
        blockId: String,
        start: Int,
        end: Int,
        map: SourceMap,
    ): Interval = absorbMarkerOnly(ownOf(all, blockId), start, end, map)

    private class Resolved(
        val range: Interval,
        val rest: List<Highlight>,
    )

    private fun resolve(
        own: List<Highlight>,
        color: HighlightColor,
        start: Int,
        end: Int,
        map: SourceMap?,
    ): Resolved {
        var s = start
        var e = end
        val rest = own.toMutableList()
        var grew = true
        while (grew) {
            grew = false
            val it = rest.iterator()
            while (it.hasNext()) {
                val h = it.next()
                if (h.color == color && h.start <= e && h.end >= s) {
                    s = minOf(s, h.start)
                    e = maxOf(e, h.end)
                    it.remove()
                    grew = true
                }
            }
            if (map != null) {
                // Widening over a marker-only remainder may reach a same colour neighbour: merge again.
                val wide = absorbMarkerOnly(rest, s, e, map)
                if (wide.start != s || wide.end != e) {
                    s = wide.start
                    e = wide.end
                    grew = true
                }
            }
        }
        return Resolved(Interval(s, e), rest)
    }

    /**
     * Widens `[start, end)` so that cutting it out of [own] leaves no remainder with nothing to show (`**`, a link's
     * `](https://...)`): those would be invisible, count for the limit and travel to the backend.
     */
    private fun absorbMarkerOnly(
        own: List<Highlight>,
        start: Int,
        end: Int,
        map: SourceMap,
    ): Interval {
        var s = start
        var e = end
        var changed = true
        while (changed) {
            changed = false
            for (h in own) {
                if (h.end <= s || h.start >= e) continue
                if (h.start < s && map.sourceToDisplay(Interval(h.start, s)).isEmpty()) {
                    s = h.start
                    changed = true
                }
                if (h.end > e && map.sourceToDisplay(Interval(e, h.end)).isEmpty()) {
                    e = h.end
                    changed = true
                }
            }
        }
        return Interval(s, e)
    }

    /** Removes whole every highlight of [blockId] that covers the source [position] (a tap on it). */
    fun removeAt(
        all: List<Highlight>,
        blockId: String,
        position: Int,
    ): List<Highlight> {
        val own = ownOf(all, blockId)
        return replaceBlock(all, blockId, own.filterNot { position >= it.start && position < it.end })
    }

    /** The highlight of [blockId] covering the source [position], if any. */
    fun at(
        all: List<Highlight>,
        blockId: String,
        position: Int,
    ): Highlight? = all.firstOrNull { it.blockId == blockId && position >= it.start && position < it.end }

    /**
     * The highlight of [blockId] that overlaps the source [range] (the source interval of a tapped char), if any.
     * It is the same test the drawing uses (`SourceMap.sourceToDisplay`), so what is seen can be tapped even when it
     * starts in the middle of an atomic element (`[[key]]`, `[^k]`) written by another client.
     */
    fun overlapping(
        all: List<Highlight>,
        blockId: String,
        range: Interval,
    ): Highlight? = all.firstOrNull { it.blockId == blockId && it.start < range.end && it.end > range.start }

    private fun cut(
        h: Highlight,
        s: Int,
        e: Int,
    ): List<Highlight> {
        if (h.end <= s || h.start >= e) return listOf(h)
        return listOfNotNull(
            if (h.start < s) h.copy(end = s) else null,
            if (h.end > e) h.copy(start = e) else null,
        )
    }

    /** The entries of [blockId] sorted by start, with overlaps repaired (the earlier one keeps the shared part). */
    private fun ownOf(
        all: List<Highlight>,
        blockId: String,
    ): List<Highlight> {
        val sorted = all.filter { it.blockId == blockId && it.start >= 0 && it.start < it.end }.sortedBy { it.start }
        val out = mutableListOf<Highlight>()
        for (h in sorted) {
            val last = out.lastOrNull()
            when {
                last == null || h.start >= last.end -> out += h
                h.end > last.end -> out += h.copy(start = last.end)
            }
        }
        return out
    }

    private fun replaceBlock(
        all: List<Highlight>,
        blockId: String,
        own: List<Highlight>,
    ): List<Highlight> {
        val sorted = own.sortedBy { it.start }
        val at = all.indexOfFirst { it.blockId == blockId }
        if (at < 0) return all + sorted
        val out = mutableListOf<Highlight>()
        all.forEachIndexed { i, h ->
            if (i == at) out += sorted
            if (h.blockId != blockId) out += h
        }
        return out
    }
}
