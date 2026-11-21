package com.teachme.sdui.ink

import kotlin.math.hypot

/** The rectangle where the pen writes (handwriting.md section 5.1), in the pointer's space. Edges are inside. */
data class WritingArea(
    val left: Float,
    val top: Float,
    val right: Float,
    val bottom: Float,
) {
    fun contains(
        x: Float,
        y: Float,
    ): Boolean = x in left..right && y in top..bottom
}

/**
 * Builds the strokes of one pen contact. The block is decided once by the caller (see [pickBlock]) and never changes.
 *
 * - A point outside the [area] or outside the allowed contract ranges closes the stroke for good: later points are
 *   ignored (a new contact needs a new builder).
 * - When a point arrives and the current stroke already has [InkLimits.SPLIT_AT_POINTS] raw points, that stroke is
 *   closed and a new one with the same style and block starts from the last point (so the line stays continuous).
 * - A closed stroke is simplified (Ramer-Douglas-Peucker, [InkLimits.SIMPLIFY_EPSILON_DP]) and quantized to the
 *   contract integers. Consecutive equal points collapse; a tap is one point.
 *
 * Finished strokes are collected with [drain] (during the gesture, so they can be added to the set) and [finish].
 */
class StrokeBuilder private constructor(
    private val blockId: String,
    private val geometry: BlockGeometry,
    private val color: InkColor,
    private val width: InkWidth,
    private val area: WritingArea,
    private val epsilonPx: Float,
    private val newId: () -> String,
) {
    private val raw = mutableListOf<InkPoint>()
    private val done = mutableListOf<Stroke>()

    /** True until a point closes the stroke or [finish] is called. */
    var isOpen: Boolean = true
        private set

    /** The raw points of the stroke being drawn, for the live rendering. */
    val livePoints: List<InkPoint> get() = raw

    private fun accepts(
        x: Float,
        y: Float,
    ): Boolean = area.contains(x, y) && geometry.toContract(x, y) != null

    /** Adds a point. Returns false (and closes the stroke) when it is out of the area or the allowed ranges. */
    fun add(
        x: Float,
        y: Float,
    ): Boolean {
        if (!isOpen) return false
        if (!accepts(x, y)) {
            close()
            return false
        }
        if (raw.size >= InkLimits.SPLIT_AT_POINTS) {
            val last = raw.last()
            emit()
            raw.clear()
            raw.add(last)
        }
        raw.add(InkPoint(x, y))
        return true
    }

    /** The strokes finished so far and not yet taken. */
    fun drain(): List<Stroke> = done.toList().also { done.clear() }

    /** Closes the stroke (the pen went up) and returns everything not yet drained. */
    fun finish(): List<Stroke> {
        close()
        return drain()
    }

    private fun close() {
        if (!isOpen) return
        isOpen = false
        emit()
        raw.clear()
    }

    private fun emit() {
        if (raw.isEmpty()) return
        val simplified = simplify(raw, epsilonPx)
        val flat = ArrayList<Int>(simplified.size * 2)
        var lastX = Int.MIN_VALUE
        var lastY = Int.MIN_VALUE
        for (p in simplified) {
            val c = geometry.toContract(p.x, p.y) ?: continue
            if (c.x == lastX && c.y == lastY) continue
            flat += c.x
            flat += c.y
            lastX = c.x
            lastY = c.y
        }
        if (flat.isNotEmpty()) done += Stroke(newId(), blockId, color, width, flat)
    }

    companion object {
        /**
         * Starts a stroke at the pen-down point. Null when that point is out of the area or the allowed ranges (the
         * contact then writes nothing). [density] is pixels per dp.
         */
        fun start(
            blockId: String,
            geometry: BlockGeometry,
            color: InkColor,
            width: InkWidth,
            area: WritingArea,
            density: Float,
            x: Float,
            y: Float,
            newId: () -> String,
        ): StrokeBuilder? {
            val builder =
                StrokeBuilder(blockId, geometry, color, width, area, InkLimits.SIMPLIFY_EPSILON_DP * density, newId)
            return if (builder.add(x, y)) builder else null
        }
    }
}

/** Ramer-Douglas-Peucker with an explicit stack. The first and last points are always kept. */
internal fun simplify(
    points: List<InkPoint>,
    epsilon: Float,
): List<InkPoint> {
    if (points.size < 3) return points.toList()
    val keep = BooleanArray(points.size)
    keep[0] = true
    keep[points.lastIndex] = true
    val stack = ArrayDeque<IntArray>()
    stack.addLast(intArrayOf(0, points.lastIndex))
    while (stack.isNotEmpty()) {
        val (from, to) = stack.removeLast()
        var farthest = -1
        var farthestDistance = epsilon
        for (i in from + 1 until to) {
            val d = distanceToSegment(points[i], points[from], points[to])
            if (d > farthestDistance) {
                farthest = i
                farthestDistance = d
            }
        }
        if (farthest >= 0) {
            keep[farthest] = true
            stack.addLast(intArrayOf(from, farthest))
            stack.addLast(intArrayOf(farthest, to))
        }
    }
    return points.filterIndexed { i, _ -> keep[i] }
}

internal fun distanceToSegment(
    p: InkPoint,
    a: InkPoint,
    b: InkPoint,
): Float {
    val dx = b.x - a.x
    val dy = b.y - a.y
    val lengthSquared = dx * dx + dy * dy
    if (lengthSquared == 0f) return hypot(p.x - a.x, p.y - a.y)
    val t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSquared).coerceIn(0f, 1f)
    return hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy))
}
