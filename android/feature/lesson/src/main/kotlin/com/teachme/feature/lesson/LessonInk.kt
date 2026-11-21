package com.teachme.feature.lesson

import android.view.MotionEvent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.changedToDownIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntSize
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.ink.BlockExtent
import com.teachme.sdui.ink.BlockGeometry
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkEdit
import com.teachme.sdui.ink.InkEraser
import com.teachme.sdui.ink.InkPoint
import com.teachme.sdui.ink.InkWidth
import com.teachme.sdui.ink.Stroke
import com.teachme.sdui.ink.StrokeBoundsCache
import com.teachme.sdui.ink.StrokeBuilder
import com.teachme.sdui.ink.WritingArea
import com.teachme.sdui.ink.penErases
import com.teachme.sdui.ink.pickBlock
import java.util.UUID
import kotlin.math.hypot
import androidx.compose.ui.graphics.drawscope.Stroke as StrokeStyle

/**
 * Where the top-level blocks are now, in the space of the lesson box (the one the pen events and the ink layers share):
 * the geometry of each measured block, their vertical extents in document order, and the "Continua" row, where the pen
 * keeps its normal job (handwriting.md sections 5.2 and 7).
 */
internal data class InkLayout(
    val blocks: Map<String, BlockGeometry> = emptyMap(),
    val extents: List<BlockExtent> = emptyList(),
    val continueTop: Float = Float.NaN,
    val continueBottom: Float = Float.NaN,
) {
    fun inContinue(y: Float): Boolean = !continueTop.isNaN() && y >= continueTop && y <= continueBottom
}

/**
 * The heights of the list rows seen so far, per row key, for the current column width. A block that has just scrolled
 * off the screen is not measured any more, but its notes can still reach into view: its place is worked out from the
 * rows around it with these heights (handwriting.md section 11, the neighbours of the visible blocks).
 */
internal class InkRowSizes {
    private val sizes = HashMap<String, Int>()
    private var width = Float.NaN

    operator fun get(key: String): Int? = sizes[key]

    fun put(
        key: String,
        size: Int,
    ) {
        sizes[key] = size
    }

    /** A new column width means every height is out of date. */
    fun forgetUnless(columnWidth: Float) {
        if (columnWidth != width) {
            sizes.clear()
            width = columnWidth
        }
    }
}

/**
 * The layout from the list's measured items. [pads] are the top paddings (pixels) of [items] inside their rows: the
 * block starts under its gap. A row that is not measured and whose height was never seen gets no geometry, so its ink
 * is not drawn until it is. Rows next to the visible ones are placed with the heights remembered in [rowSizes]. Blocks
 * without an id cannot carry ink and are left out.
 */
internal fun computeInkLayout(
    info: LazyListLayoutInfo,
    items: List<LessonItem>,
    pads: List<Float>,
    left: Float,
    width: Float,
    rowSizes: InkRowSizes = InkRowSizes(),
): InkLayout {
    rowSizes.forgetUnless(width)
    val blocks = HashMap<String, BlockGeometry>()
    val visible = ArrayList<BlockExtent>()
    var continueTop = Float.NaN
    var continueBottom = Float.NaN
    // Offsets are from where the content starts, after the padding before it.
    val origin = info.beforeContentPadding
    var firstIndex = Int.MAX_VALUE
    var firstOffset = 0
    var lastIndex = -1
    var lastEnd = 0

    fun place(
        item: LessonItem.BlockItem,
        index: Int,
        rowOffset: Int,
        rowSize: Int,
        into: MutableList<BlockExtent>,
    ) {
        val id = item.block.id
        if (id.isEmpty()) return
        val top = origin + rowOffset + (pads.getOrNull(index) ?: 0f)
        blocks[id] = BlockGeometry(left, top, width)
        into += BlockExtent(id, top, (origin + rowOffset + rowSize - top).coerceAtLeast(0f))
    }
    for (row in info.visibleItemsInfo) {
        val item = items.getOrNull(row.index)?.takeIf { it.key == row.key } ?: continue
        rowSizes.put(row.key as String, row.size)
        if (row.index < firstIndex) {
            firstIndex = row.index
            firstOffset = row.offset
        }
        if (row.index > lastIndex) {
            lastIndex = row.index
            lastEnd = row.offset + row.size
        }
        when (item) {
            is LessonItem.BlockItem -> place(item, row.index, row.offset, row.size, visible)
            is LessonItem.Continue -> {
                continueTop = (origin + row.offset).toFloat()
                continueBottom = (origin + row.offset + row.size).toFloat()
            }
            else -> Unit
        }
    }
    // The rows above and below the measured ones, as far as their heights are known.
    val before = ArrayList<BlockExtent>()
    val after = ArrayList<BlockExtent>()
    if (lastIndex >= 0) {
        var offset = firstOffset
        for (i in firstIndex - 1 downTo 0) {
            val size = rowSizes[items[i].key] ?: break
            offset -= size
            (items[i] as? LessonItem.BlockItem)?.let { place(it, i, offset, size, before) }
        }
        offset = lastEnd
        for (i in lastIndex + 1 until items.size) {
            val size = rowSizes[items[i].key] ?: break
            (items[i] as? LessonItem.BlockItem)?.let { place(it, i, offset, size, after) }
            offset += size
        }
    }
    return InkLayout(blocks, before.reversed() + visible + after, continueTop, continueBottom)
}

private fun PointerType.isPen() = this == PointerType.Stylus || this == PointerType.Eraser

/** The side button of a stylus, read from the native event: Compose maps it to no button of its own. */
private fun PointerEvent.stylusButtonPressed(): Boolean =
    motionEvent?.let { it.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY != 0 } == true

/** The width of a pen width in dp: the design token (the same in dp at any scale of the note). */
internal fun InkWidth.penDp(): Float =
    when (this) {
        InkWidth.THIN -> TeachHandwriting.widthThin.value
        InkWidth.MEDIUM -> TeachHandwriting.widthMedium.value
        InkWidth.THICK -> TeachHandwriting.widthThick.value
    }

/** The colour of an ink role in the current theme (the role is saved, not the hex). */
internal fun InkColor.paint(colors: TeachColors): Color =
    when (this) {
        InkColor.INK -> colors.penInk
        InkColor.BLUE -> colors.penBlue
        InkColor.RED -> colors.penRed
        InkColor.GREEN -> colors.penGreen
    }

/** The path of a stroke with two or more points, in pixels from the block's top-left corner (quadratics through midpoints). */
internal fun strokePath(
    points: List<Int>,
    blockWidth: Float,
): Path {
    val path = Path()
    val count = points.size / 2

    fun x(i: Int) = points[i * 2].toFloat() / TeachHandwriting.coordScale * blockWidth

    fun y(i: Int) = points[i * 2 + 1].toFloat() / TeachHandwriting.coordScale * blockWidth
    path.moveTo(x(0), y(0))
    for (i in 1 until count - 1) {
        path.quadraticTo(x(i), y(i), (x(i) + x(i + 1)) / 2f, (y(i) + y(i + 1)) / 2f)
    }
    if (count > 1) path.lineTo(x(count - 1), y(count - 1))
    return path
}

/** Paths of saved strokes, kept while the block width is the same: only a change of scale builds them again. */
internal class InkPathCache {
    private class Entry(
        var points: List<Int>,
        val blockWidth: Float,
        val path: Path,
    )

    private val entries = HashMap<String, Entry>()

    /** Number of paths built (a test hook). */
    internal var builds = 0
        private set

    fun pathOf(
        stroke: Stroke,
        blockWidth: Float,
    ): Path {
        val cached = entries[stroke.id]
        if (cached != null && cached.blockWidth == blockWidth) {
            if (cached.points === stroke.points) return cached.path
            if (cached.points == stroke.points) {
                // Equal points in a new list (after a refresh): keep the new one so the next frame is an identity check.
                cached.points = stroke.points
                return cached.path
            }
        }
        builds++
        return strokePath(stroke.points, blockWidth).also { entries[stroke.id] = Entry(stroke.points, blockWidth, it) }
    }

    /** Forgets the paths of strokes that are no longer in [alive]. */
    fun retain(alive: Set<String>) {
        if (entries.size > alive.size + CACHE_SLACK) entries.keys.retainAll(alive)
    }

    val size: Int get() = entries.size

    private companion object {
        const val CACHE_SLACK = 32
    }
}

/**
 * Everything the handwriting layer of the lesson screen holds: the settings it was given, the pen contact in progress
 * and what it shows. The contact moves only draw state (a tick, a path), never composition state that a composable
 * reads, so a stroke costs no recomposition per point (handwriting.md section 11).
 */
@Stable
internal class InkSurface(
    private val write: (Stroke) -> InkEdit,
    private val erase: (Collection<String>) -> InkEdit,
    private val newId: () -> String = { UUID.randomUUID().toString() },
) {
    // Set at every composition by the screen.
    var penWrites: Boolean = false

    /** The "Gomma" switch (handwriting.md 3.4): the stylus tip erases like the physical eraser, with either tool. */
    var eraserMode: Boolean
        get() = eraserOn
        set(value) {
            eraserOn = value
            // An old position must not come back when the switch is turned on again.
            if (!value) ring = null
        }
    private var eraserOn: Boolean by mutableStateOf(false)
    var color: InkColor = InkColor.INK
    var width: InkWidth = InkWidth.MEDIUM
    var strokes: List<Stroke> by mutableStateOf(emptyList())
    var layout: () -> InkLayout = { InkLayout() }

    /** Called when a contact cannot write because the set is at a limit (the screen shows the toast of section 8). */
    var onLimit: () -> Unit = {}

    /** Right edge of the writing area in the box (the box width when there is no context panel). */
    var areaRight: Float by mutableStateOf(Float.MAX_VALUE)

    /** Where the stylus tip is (hover or contact) while the switch is on: the cursor ring of section 3.4 is drawn there. */
    private var ring: Offset? by mutableStateOf(null)

    /** The cursor ring position, null when hidden (a test hook). */
    internal val ringAt: Offset? get() = ring

    /** Strokes the eraser touches now: drawn at the ghost opacity until the tip goes up. */
    var erasing: Set<String> by mutableStateOf(emptySet())
        private set

    /** Strokes just written and not yet in [strokes]: they stay on screen so the pen-up never blinks. */
    var pending: List<Stroke> by mutableStateOf(emptyList())
        private set

    // The stroke being written. Only the draw phase reads these.
    private val liveTick = mutableIntStateOf(0)
    private val livePath = Path()
    private var liveColor = InkColor.INK
    private var liveWidthDp = InkWidth.MEDIUM.penDp()
    private var liveActive = false
    private var liveLast = Offset.Zero
    private var liveMid = Offset.Zero
    private var liveCount = 0

    private fun startLive(
        at: InkPoint,
        color: InkColor,
        width: InkWidth,
    ) {
        liveColor = color
        liveWidthDp = width.penDp()
        liveActive = true
        restartLive(at)
    }

    private fun restartLive(at: InkPoint) {
        livePath.reset()
        livePath.moveTo(at.x, at.y)
        liveLast = Offset(at.x, at.y)
        liveMid = liveLast
        liveCount = 1
        liveTick.intValue++
    }

    private fun extendLive(at: InkPoint) {
        val p = Offset(at.x, at.y)
        val mid = (liveLast + p) / 2f
        livePath.quadraticTo(liveLast.x, liveLast.y, mid.x, mid.y)
        liveMid = mid
        liveLast = p
        liveCount++
        liveTick.intValue++
    }

    private fun stopLive() {
        liveActive = false
        liveTick.intValue++
    }

    /** True while the stroke in progress is drawn on its own layer (a test hook). */
    internal val liveVisible: Boolean get() = liveActive

    /** The stroke in progress, on its own layer: a path drawn as it grows, or a dot while the pen has not moved. */
    fun DrawScope.drawLive(colors: TeachColors) {
        liveTick.intValue
        if (!liveActive) return
        val paint = liveColor.paint(colors)
        val widthPx = liveWidthDp * density
        if (liveCount == 1) {
            drawCircle(paint, widthPx / 2f, liveLast)
            return
        }
        val style = StrokeStyle(widthPx, cap = StrokeCap.Round, join = StrokeJoin.Round)
        drawPath(livePath, paint, style = style)
        drawLine(paint, liveMid, liveLast, widthPx, StrokeCap.Round)
    }

    internal val cache = InkPathCache()
    private val bounds = StrokeBoundsCache()

    // What to draw, worked out only when [strokes] or [pending] change (not at every scroll frame).
    private var shownStrokes: List<Stroke>? = null
    private var shownPending: List<Stroke>? = null
    private var shown: List<Stroke> = emptyList()

    /** Number of strokes drawn by the last frame (a test hook). */
    internal var drawnCount = 0
        private set

    private fun shownNow(): List<Stroke> {
        val s = strokes
        val p = pending
        if (s !== shownStrokes || p !== shownPending) {
            val have = s.mapTo(HashSet()) { it.id }
            shown = if (p.isEmpty()) s else s + p.filter { it.id !in have }
            shownStrokes = s
            shownPending = p
            val alive = HashSet<String>(shown.size * 2)
            for (stroke in shown) alive += stroke.id
            cache.retain(alive)
            bounds.retain(alive)
        }
        return shown
    }

    /** The saved strokes with the current geometry of their blocks, over the text and the highlights. */
    fun DrawScope.drawSaved(colors: TeachColors) {
        val current = layout()
        val all = shownNow()
        val ghost = erasing
        val right = minOf(areaRight, size.width)
        var drawn = 0
        clipRect(0f, 0f, right, size.height) {
            for (stroke in all) {
                val g = current.blocks[stroke.blockId] ?: continue
                val pad = stroke.width.penDp() * density
                if (!bounds.intersects(stroke, g, pad, 0f, 0f, right, size.height)) continue
                drawn++
                val alpha = if (stroke.id in ghost) TeachHandwriting.eraserGhostAlpha else 1f
                val paint = stroke.color.paint(colors)
                val widthPx = stroke.width.penDp() * density
                translate(g.left, g.top) {
                    if (stroke.pointCount == 1) {
                        val p = g.toScreen(stroke.points[0], stroke.points[1])
                        drawCircle(paint, widthPx / 2f, Offset(p.x - g.left, p.y - g.top), alpha)
                    } else {
                        val style = StrokeStyle(widthPx, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        drawPath(cache.pathOf(stroke, g.width), paint, alpha, style)
                    }
                }
            }
        }
        drawnCount = drawn
    }

    /**
     * The cursor ring of the "Gomma" switch: the contact area of the eraser (`2 * eraserRadius` across), an outline in
     * `ink`, no fill, no animation, clipped like the ink. Not shown for the physical eraser or the side button.
     */
    fun DrawScope.drawRing(colors: TeachColors) {
        val at = ring ?: return
        if (!eraserMode) return
        clipRect(0f, 0f, minOf(areaRight, size.width), size.height) {
            drawCircle(
                colors.ink,
                TeachHandwriting.eraserRadius * density,
                at,
                style = StrokeStyle(TeachHandwriting.widthThin.toPx()),
            )
        }
    }

    /** Follows the stylus tip for the ring, in hover and in contact; consumes nothing. */
    suspend fun PointerInputScope.ringLoop() {
        awaitPointerEventScope {
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val tip = event.changes.firstOrNull { it.type == PointerType.Stylus }
                ring =
                    when {
                        tip == null -> ring
                        // A pen with no hover leaves no event after the lift: the ring goes with the contact.
                        event.type == PointerEventType.Exit || event.type == PointerEventType.Release -> null
                        !eraserMode || event.stylusButtonPressed() -> null
                        !erasesAt(tip.position, size) -> null
                        else -> tip.position
                    }
            }
        }
    }

    /** Drops from [pending] what the set has taken in. */
    fun settle() {
        if (pending.isEmpty()) return
        val have = strokes.mapTo(HashSet()) { it.id }
        val rest = pending.filter { it.id !in have }
        if (rest.size != pending.size) pending = rest
    }

    // --- input ---

    private fun wants(
        at: Offset,
        size: IntSize,
    ): Boolean {
        if (!penWrites) return false
        if (!inArea(at, size)) return false
        return !layout().inContinue(at.y)
    }

    /** Where the tip erases with the switch on: the writing area but not the "Continua" row, with either tool. */
    private fun erasesAt(
        at: Offset,
        size: IntSize,
    ): Boolean = inArea(at, size) && !layout().inContinue(at.y)

    private fun inArea(
        at: Offset,
        size: IntSize,
    ): Boolean = !(at.x < 0f || at.y < 0f || at.x > minOf(areaRight, size.width.toFloat()) || at.y > size.height)

    /**
     * While the tip is down the hand rests on the screen: a finger in the writing area is not ours to answer, so it is
     * consumed (no scroll under the stroke, no exercise answered). Outside a pen contact the finger is left alone.
     */
    private fun consumeTouches(
        event: PointerEvent,
        penId: PointerId,
        size: IntSize,
    ) {
        for (c in event.changes) {
            if (c.id != penId && c.type == PointerType.Touch && inArea(c.position, size)) c.consume()
        }
    }

    // The limit toast is said once per contact, however many strokes the contact tries to save.
    private var limitTold = false

    private fun tellLimit() {
        if (limitTold) return
        limitTold = true
        onLimit()
    }

    /**
     * The pen contact loop, in the initial pass above the lesson column. With the pen tool a contact that starts in the
     * writing area is ours (consumed: the exercises, the terms and the scroll never see it); anything else goes on as
     * before. A cancelled contact leaves nothing behind.
     */
    suspend fun PointerInputScope.inkLoop() {
        awaitEachGesture {
            var event: PointerEvent
            var down: PointerInputChange?
            do {
                event = awaitPointerEvent(PointerEventPass.Initial)
                down =
                    event.changes.firstOrNull {
                        it.type.isPen() && it.changedToDownIgnoreConsumed() && wants(it.position, size)
                    }
            } while (down == null)
            limitTold = false
            val erasing = penErases(down.type, event.stylusButtonPressed(), eraserMode)
            down.consume()
            if (erasing) eraseContact(down) else writeContact(down)
        }
    }

    private suspend fun androidx.compose.ui.input.pointer.AwaitPointerEventScope.writeContact(
        down: PointerInputChange,
    ) {
        val now = layout()
        val blockId = pickBlock(now.extents, down.position.y)
        val geometry = blockId?.let { now.blocks[it] }
        val area = WritingArea(0f, 0f, minOf(areaRight, size.width.toFloat()), size.height.toFloat())
        val full =
            strokes.size >= TeachHandwriting.limitMaxStrokes ||
                strokes.sumOf { it.pointCount } >= TeachHandwriting.limitMaxPointsPerLesson
        // At the limit no stroke starts: the contact is still ours (no answer, no scroll) and says why.
        if (full && blockId != null && geometry != null) tellLimit()
        val builder =
            if (blockId == null || geometry == null || full) {
                null
            } else {
                StrokeBuilder.start(
                    blockId,
                    geometry,
                    color,
                    width,
                    area,
                    density,
                    down.position.x,
                    down.position.y,
                    newId,
                )
            }
        if (builder != null) startLive(InkPoint(down.position.x, down.position.y), color, width)
        // A release settles the stroke by itself. Any other way out (the pointer vanishes from the events, the loop is
        // torn down) keeps what was written, unless the system said it was cancelled.
        var settled = false
        try {
            while (true) {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                consumeTouches(e, down.id, size)
                val c = e.changes.firstOrNull { it.id == down.id }
                if (c == null) {
                    settled = e.motionEvent?.actionMasked == MotionEvent.ACTION_CANCEL
                    break
                }
                c.consume()
                if (c.pressed) {
                    if (builder != null && builder.isOpen) {
                        for (h in c.historical) feed(builder, h.position)
                        feed(builder, c.position)
                    }
                    continue
                }
                // Compose reports a cancel as a release with no motion event behind it: only a real UP keeps the stroke.
                settled = true
                val action = e.motionEvent?.actionMasked
                if (builder != null && action != null && action != MotionEvent.ACTION_CANCEL) commit(builder.finish())
                break
            }
        } finally {
            if (!settled && builder != null) commit(builder.finish())
            stopLive()
        }
    }

    private fun feed(
        builder: StrokeBuilder,
        at: Offset,
    ) {
        if (!builder.add(at.x, at.y)) {
            // The pen left the area: the stroke closes here and the rest of the contact writes nothing.
            commit(builder.drain())
            stopLive()
            return
        }
        val split = builder.drain()
        if (split.isNotEmpty()) {
            // Split at 800 points: the closed part is saved, the line goes on from its last point.
            commit(split)
            restartLive(builder.livePoints.first())
        }
        extendLive(InkPoint(at.x, at.y))
    }

    private fun commit(done: List<Stroke>) {
        for (stroke in done) {
            when (write(stroke)) {
                is InkEdit.Applied -> pending = pending + stroke
                is InkEdit.Rejected -> tellLimit()
                InkEdit.NoOp -> Unit
            }
        }
    }

    private suspend fun androidx.compose.ui.input.pointer.AwaitPointerEventScope.eraseContact(
        down: PointerInputChange,
    ) {
        val touched = LinkedHashSet<String>()
        var last: Offset? = null

        fun sample(at: Offset) {
            if (at.x < 0f || at.y < 0f || at.x > minOf(areaRight, size.width.toFloat()) || at.y > size.height) return
            val from = last
            last = at
            val path = ArrayList<InkPoint>()
            if (from != null) {
                // Fast moves skip over strokes: fill in the way between two samples.
                val step = TeachHandwriting.eraserRadius * density / 2f
                val distance = hypot(at.x - from.x, at.y - from.y)
                var d = step
                while (d < distance) {
                    val t = d / distance
                    path += InkPoint(from.x + (at.x - from.x) * t, from.y + (at.y - from.y) * t)
                    d += step
                }
            }
            path += InkPoint(at.x, at.y)
            val current = layout()
            val ids = InkEraser.touched(strokes, { current.blocks[it] }, path, density, bounds)
            if (touched.addAll(ids)) erasing = touched.toSet()
        }
        sample(down.position)
        try {
            while (true) {
                val e = awaitPointerEvent(PointerEventPass.Initial)
                consumeTouches(e, down.id, size)
                val c = e.changes.firstOrNull { it.id == down.id } ?: break
                c.consume()
                if (c.pressed) {
                    for (h in c.historical) sample(h.position)
                    sample(c.position)
                    continue
                }
                val action = e.motionEvent?.actionMasked
                if (action != null &&
                    action != MotionEvent.ACTION_CANCEL &&
                    touched.isNotEmpty()
                ) {
                    erase(touched.toList())
                }
                break
            }
        } finally {
            erasing = emptySet()
        }
    }
}

/** The pen input of the lesson column: place it on the box that holds the list. */
internal fun Modifier.inkInput(surface: InkSurface): Modifier =
    pointerInput(surface) { with(surface) { ringLoop() } }
        .pointerInput(surface) { with(surface) { inkLoop() } }

/**
 * The ink above the lesson column: the saved strokes on one layer and the stroke in progress on another, so a new
 * point only redraws the small one. Neither takes touches.
 */
@Composable
internal fun InkLayers(
    surface: InkSurface,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    LaunchedEffect(surface, surface.strokes) { surface.settle() }
    Canvas(modifier.fillMaxSize()) { with(surface) { drawSaved(colors) } }
    Canvas(modifier.fillMaxSize()) { with(surface) { drawLive(colors) } }
    Canvas(modifier.fillMaxSize()) { with(surface) { drawRing(colors) } }
}
