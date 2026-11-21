package com.teachme.sdui.ink

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** Limits and ranges of the handwriting contract (`lessons.md` section 11) and of the app (handwriting.md section 8). */
object InkLimits {
    const val MAX_STROKES = 2000
    const val MAX_POINTS_PER_STROKE = 1000
    const val MAX_POINTS = 200_000

    /** The app closes a stroke at this many points and goes on in a new one (`appSplitStrokeAtPoints`). */
    const val SPLIT_AT_POINTS = 800

    /** Contract coordinates are ten-thousandths of the block width. */
    const val SCALE = 10_000

    const val X_MIN = -40_000
    const val X_MAX = 50_000
    const val Y_MIN = -40_000
    const val Y_MAX = 200_000

    const val UNDO_DEPTH = 50

    /** Eraser contact radius in dp (`eraserRadius`). */
    const val ERASER_RADIUS_DP = 12f

    /** Simplification tolerance in dp (`simplifyEpsilon`). */
    const val SIMPLIFY_EPSILON_DP = 0.4f
}

/** The four ink roles. The role is saved, not the hex, so a note follows the theme. */
@Serializable(with = InkColorSerializer::class)
enum class InkColor(
    val wire: String,
) {
    INK("ink"),
    BLUE("blue"),
    RED("red"),
    GREEN("green"),
    ;

    companion object {
        /** An unknown value is ink, as the contract asks. */
        fun fromWire(value: String?): InkColor = entries.firstOrNull { it.wire == value } ?: INK
    }
}

/** The three pen widths, constant in dp whatever the scale of the note. */
@Serializable(with = InkWidthSerializer::class)
enum class InkWidth(
    val wire: String,
    val dp: Float,
) {
    THIN("thin", 1.5f),
    MEDIUM("medium", 2.5f),
    THICK("thick", 4f),
    ;

    companion object {
        /** An unknown value is medium, as the contract asks. */
        fun fromWire(value: String?): InkWidth = entries.firstOrNull { it.wire == value } ?: MEDIUM
    }
}

object InkColorSerializer : KSerializer<InkColor> {
    override val descriptor = PrimitiveSerialDescriptor("InkColor", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder) = InkColor.fromWire(decoder.decodeString())

    override fun serialize(
        encoder: Encoder,
        value: InkColor,
    ) = encoder.encodeString(value.wire)
}

object InkWidthSerializer : KSerializer<InkWidth> {
    override val descriptor = PrimitiveSerialDescriptor("InkWidth", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder) = InkWidth.fromWire(decoder.decodeString())

    override fun serialize(
        encoder: Encoder,
        value: InkWidth,
    ) = encoder.encodeString(value.wire)
}

/**
 * A pen stroke on a top-level block. [points] is the flat `[x0, y0, x1, y1, ...]` of the contract, in ten-thousandths
 * of the block width from its top-left corner.
 */
@Serializable
data class Stroke(
    val id: String,
    @SerialName("block_id") val blockId: String,
    val color: InkColor,
    val width: InkWidth,
    val points: List<Int>,
) {
    val pointCount: Int get() = points.size / 2
}

/** Which limit an addition would break. */
enum class InkLimit { TOO_MANY_STROKES, TOO_MANY_POINTS_IN_STROKE, TOO_MANY_POINTS }

/** A stroke and its position in the drawing order. */
data class IndexedStroke(
    val index: Int,
    val stroke: Stroke,
)

/** The outcome of changing an [InkSet]: the new set, or the limit that refused it (the set is then unchanged). */
sealed interface InkSetResult {
    data class Ok(
        val set: InkSet,
    ) : InkSetResult

    data class Rejected(
        val limit: InkLimit,
    ) : InkSetResult
}

/**
 * The ordered strokes of a lesson (the order is the drawing order, the first is at the bottom), within the contract
 * limits. Immutable: every change returns a new set.
 */
class InkSet private constructor(
    val strokes: List<Stroke>,
    val totalPoints: Int,
) {
    val size: Int get() = strokes.size

    /** Appends [stroke] on top, or says which limit it would break. */
    fun add(stroke: Stroke): InkSetResult = insert(listOf(IndexedStroke(strokes.size, stroke)))

    /**
     * Puts each entry at its [IndexedStroke.index] (clamped), processed in ascending index order: giving the original
     * positions of removed strokes restores the original order.
     */
    fun insert(entries: List<IndexedStroke>): InkSetResult {
        if (strokes.size + entries.size > InkLimits.MAX_STROKES) return InkSetResult.Rejected(InkLimit.TOO_MANY_STROKES)
        if (entries.any { it.stroke.pointCount > InkLimits.MAX_POINTS_PER_STROKE }) {
            return InkSetResult.Rejected(InkLimit.TOO_MANY_POINTS_IN_STROKE)
        }
        val points = totalPoints + entries.sumOf { it.stroke.pointCount }
        if (points > InkLimits.MAX_POINTS) return InkSetResult.Rejected(InkLimit.TOO_MANY_POINTS)
        val result = strokes.toMutableList()
        for (entry in entries.sortedBy { it.index }) result.add(entry.index.coerceIn(0, result.size), entry.stroke)
        return InkSetResult.Ok(InkSet(result, points))
    }

    /** The strokes with these ids, with their positions. */
    fun entriesOf(ids: Set<String>): List<IndexedStroke> =
        strokes.mapIndexedNotNull { i, s -> if (s.id in ids) IndexedStroke(i, s) else null }

    fun remove(ids: Set<String>): InkSet {
        val kept = strokes.filter { it.id !in ids }
        return if (kept.size == strokes.size) this else InkSet(kept, kept.sumOf { it.pointCount })
    }

    companion object {
        val EMPTY = InkSet(emptyList(), 0)

        /**
         * A set from stored strokes (the server's, or the disk's), in order. It does not refuse: what is over a limit
         * was already accepted elsewhere, and dropping it here would lose data.
         */
        fun of(strokes: List<Stroke>): InkSet = InkSet(strokes, strokes.sumOf { it.pointCount })
    }
}
