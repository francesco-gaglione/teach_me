package com.teachme.sdui.ink

import kotlin.math.hypot

/**
 * The bounding box of each stroke in contract coordinates, worked out once per stroke and not per sample or frame.
 * The box does not depend on the block geometry, so a re-layout keeps it. An entry is valid while the stroke has the
 * same points (same list, or equal by value after a refresh: the new list is then kept so identity holds next time).
 */
class StrokeBoundsCache {
    private class Entry(
        var points: List<Int>,
        val minX: Int,
        val minY: Int,
        val maxX: Int,
        val maxY: Int,
    )

    private val entries = HashMap<String, Entry>()

    /** Number of boxes worked out (a test hook: scanning the points is the cost this cache avoids). */
    internal var computed = 0
        private set

    /** Number of segment distances measured by the eraser (a test hook). */
    internal var segmentTests = 0L

    private fun entryOf(stroke: Stroke): Entry {
        val cached = entries[stroke.id]
        if (cached != null) {
            if (cached.points === stroke.points) return cached
            if (cached.points == stroke.points) {
                cached.points = stroke.points
                return cached
            }
        }
        computed++
        val pts = stroke.points
        var minX = Int.MAX_VALUE
        var minY = Int.MAX_VALUE
        var maxX = Int.MIN_VALUE
        var maxY = Int.MIN_VALUE
        for (i in 0 until pts.size / 2) {
            val x = pts[i * 2]
            val y = pts[i * 2 + 1]
            if (x < minX) minX = x
            if (x > maxX) maxX = x
            if (y < minY) minY = y
            if (y > maxY) maxY = y
        }
        return Entry(pts, minX, minY, maxX, maxY).also { entries[stroke.id] = it }
    }

    /**
     * True when the box of [stroke] on the screen, grown by [padding] on every side, meets the rectangle
     * [left], [top], [right], [bottom]. A stroke with no points never does.
     */
    fun intersects(
        stroke: Stroke,
        geometry: BlockGeometry,
        padding: Float,
        left: Float,
        top: Float,
        right: Float,
        bottom: Float,
    ): Boolean {
        if (stroke.pointCount == 0) return false
        val e = entryOf(stroke)
        val x0 = geometry.left + e.minX.toFloat() / InkLimits.SCALE * geometry.width
        val x1 = geometry.left + e.maxX.toFloat() / InkLimits.SCALE * geometry.width
        val y0 = geometry.top + e.minY.toFloat() / InkLimits.SCALE * geometry.width
        val y1 = geometry.top + e.maxY.toFloat() / InkLimits.SCALE * geometry.width
        return minOf(x0, x1) - padding <= right &&
            maxOf(x0, x1) + padding >= left &&
            minOf(y0, y1) - padding <= bottom &&
            maxOf(y0, y1) + padding >= top
    }

    /** Forgets the boxes of strokes that are no longer in [alive]. */
    fun retain(alive: Set<String>) {
        if (entries.size > alive.size + SLACK) entries.keys.retainAll(alive)
    }

    internal val size: Int get() = entries.size

    private companion object {
        const val SLACK = 32
    }
}

/** The strokes the eraser touches (handwriting.md section 6.1). */
object InkEraser {
    /**
     * The ids (in drawing order) of the strokes touched by an eraser pass. A stroke is touched when the distance from a
     * sample of [path] to its line is at most the eraser radius plus half its width. Any stroke counts, of any block;
     * one whose block has no geometry (not measured) is skipped. [density] is pixels per dp. A stroke whose box (grown
     * by the reach) does not meet the box of the samples is dropped before any distance is measured; pass the same
     * [bounds] on every call of a pass so the boxes are worked out once.
     */
    fun touched(
        strokes: List<Stroke>,
        geometryOf: (String) -> BlockGeometry?,
        path: List<InkPoint>,
        density: Float,
        bounds: StrokeBoundsCache = StrokeBoundsCache(),
    ): List<String> {
        if (path.isEmpty()) return emptyList()
        var pl = Float.MAX_VALUE
        var pt = Float.MAX_VALUE
        var pr = -Float.MAX_VALUE
        var pb = -Float.MAX_VALUE
        for (s in path) {
            if (s.x < pl) pl = s.x
            if (s.x > pr) pr = s.x
            if (s.y < pt) pt = s.y
            if (s.y > pb) pb = s.y
        }
        val result = ArrayList<String>()
        for (stroke in strokes) {
            val geometry = geometryOf(stroke.blockId) ?: continue
            val reach = (InkLimits.ERASER_RADIUS_DP + stroke.width.dp / 2f) * density
            if (!bounds.intersects(stroke, geometry, reach, pl, pt, pr, pb)) continue
            if (hits(stroke, geometry, path, reach, bounds)) result += stroke.id
        }
        return result
    }

    private fun hits(
        stroke: Stroke,
        g: BlockGeometry,
        path: List<InkPoint>,
        reach: Float,
        bounds: StrokeBoundsCache,
    ): Boolean {
        val pts = stroke.points
        val count = stroke.pointCount
        if (count == 0) return false
        for (sample in path) {
            if (count == 1) {
                bounds.segmentTests++
                if (segment(sample, g, pts, 0, 0) <= reach) return true
                continue
            }
            for (i in 0 until count - 1) {
                bounds.segmentTests++
                if (segment(sample, g, pts, i, i + 1) <= reach) return true
            }
        }
        return false
    }

    /** Distance from [p] to the segment between points [a] and [b] of [pts], with no allocation. */
    private fun segment(
        p: InkPoint,
        g: BlockGeometry,
        pts: List<Int>,
        a: Int,
        b: Int,
    ): Float {
        val ax = g.left + pts[a * 2].toFloat() / InkLimits.SCALE * g.width
        val ay = g.top + pts[a * 2 + 1].toFloat() / InkLimits.SCALE * g.width
        val bx = g.left + pts[b * 2].toFloat() / InkLimits.SCALE * g.width
        val by = g.top + pts[b * 2 + 1].toFloat() / InkLimits.SCALE * g.width
        val dx = bx - ax
        val dy = by - ay
        val lengthSquared = dx * dx + dy * dy
        if (lengthSquared == 0f) return hypot(p.x - ax, p.y - ay)
        val t = (((p.x - ax) * dx + (p.y - ay) * dy) / lengthSquared).coerceIn(0f, 1f)
        return hypot(p.x - (ax + t * dx), p.y - (ay + t * dy))
    }
}

/** An undoable operation: a written stroke or an eraser pass (with all the strokes it took). */
sealed interface InkOperation {
    data class Write(
        val entry: IndexedStroke,
    ) : InkOperation

    data class Erase(
        val entries: List<IndexedStroke>,
    ) : InkOperation
}

/** The outcome of an edit on [InkHistory]. */
sealed interface InkEdit {
    data class Applied(
        val history: InkHistory,
    ) : InkEdit

    /** The edit would break a limit: nothing changed (stacks included). */
    data class Rejected(
        val limit: InkLimit,
    ) : InkEdit

    /** Nothing to do (empty eraser pass, empty stack). */
    data object NoOp : InkEdit
}

/**
 * The strokes of a lesson with the Undo/Redo stacks of the screen session (handwriting.md section 6.2). Immutable.
 * Undo of an erase puts the strokes back at their original positions. The undo stack keeps [InkLimits.UNDO_DEPTH]
 * operations (the oldest is dropped silently); a new operation empties Redo.
 */
class InkHistory(
    val set: InkSet = InkSet.EMPTY,
    val undoStack: List<InkOperation> = emptyList(),
    val redoStack: List<InkOperation> = emptyList(),
) {
    val canUndo: Boolean get() = undoStack.isNotEmpty()
    val canRedo: Boolean get() = redoStack.isNotEmpty()

    /** The operation Undo would take back now, if any. */
    val lastOperation: InkOperation? get() = undoStack.lastOrNull()

    /** Writes a finished stroke on top. */
    fun write(stroke: Stroke): InkEdit {
        val position = set.size
        return when (val result = set.add(stroke)) {
            is InkSetResult.Rejected -> InkEdit.Rejected(result.limit)
            is InkSetResult.Ok -> InkEdit.Applied(next(result.set, InkOperation.Write(IndexedStroke(position, stroke))))
        }
    }

    /** One eraser pass over the strokes [ids] (those of [InkEraser.touched]). */
    fun erase(ids: Collection<String>): InkEdit {
        val entries = set.entriesOf(ids.toSet())
        if (entries.isEmpty()) return InkEdit.NoOp
        return InkEdit.Applied(next(set.remove(entries.map { it.stroke.id }.toSet()), InkOperation.Erase(entries)))
    }

    fun undo(): InkEdit {
        val op = undoStack.lastOrNull() ?: return InkEdit.NoOp
        val changed =
            when (op) {
                is InkOperation.Write -> InkSetResult.Ok(set.remove(setOf(op.entry.stroke.id)))
                is InkOperation.Erase -> set.insert(op.entries)
            }
        return when (changed) {
            is InkSetResult.Rejected -> InkEdit.Rejected(changed.limit)
            is InkSetResult.Ok -> InkEdit.Applied(InkHistory(changed.set, undoStack.dropLast(1), redoStack + op))
        }
    }

    fun redo(): InkEdit {
        val op = redoStack.lastOrNull() ?: return InkEdit.NoOp
        val changed =
            when (op) {
                is InkOperation.Write -> set.insert(listOf(op.entry))
                is InkOperation.Erase -> InkSetResult.Ok(set.remove(op.entries.map { it.stroke.id }.toSet()))
            }
        return when (changed) {
            is InkSetResult.Rejected -> InkEdit.Rejected(changed.limit)
            is InkSetResult.Ok ->
                InkEdit.Applied(
                    InkHistory(changed.set, (undoStack + op).takeLast(InkLimits.UNDO_DEPTH), redoStack.dropLast(1)),
                )
        }
    }

    private fun next(
        newSet: InkSet,
        op: InkOperation,
    ) = InkHistory(newSet, (undoStack + op).takeLast(InkLimits.UNDO_DEPTH), emptyList())
}
