package com.teachme.sdui.highlight

import android.view.MotionEvent
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.changedToDownIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.raisedShadow
import com.teachme.sdui.R
import com.teachme.sdui.ink.penErases
import com.teachme.sdui.inline.BuiltInline
import com.teachme.sdui.inline.InlineAction
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.math.abs

private const val A11Y_WORDS = 6

/** A "Rimuovi" popover being shown for [highlight]; [x], [lineTop] and [lineBottom] are in the text node. */
private class PopoverState(
    val highlight: Highlight,
    val x: Float,
    val lineTop: Float,
    val lineBottom: Float,
    val openedAtY: Float,
)

/**
 * Everything the highlighter adds to one text block: the drawing under the text, the pen and tap input, the screen
 * reader description and the "Rimuovi" popover. The block keeps using the shared `InlineText`; it only asks for this.
 */
internal class HighlightSupport(
    private val built: BuiltInline,
    var target: HighlightTarget,
) {
    var host: HighlightHost = NoHighlights
    var layout: () -> TextLayoutResult? = { null }
    lateinit var colors: TeachColors
    private var lastY = 0f
    private var stroke by mutableStateOf<StrokeUi?>(null)
    private var popover by mutableStateOf<PopoverState?>(null)
    private val map = requireNotNull(built.sourceMap) { "highlightable text is built with a source map" }
    private val words = WordIndex.of(map)
    private val blockId get() = target.blockId

    private fun own(): List<Highlight> = host.highlights.filter { it.blockId == blockId }

    val drawModifier: Modifier =
        Modifier.drawBehind {
            val l = layout() ?: return@drawBehind
            val own = own()
            val selected = popover?.highlight
            // The popover's highlight may be gone (removed from elsewhere): draw nothing for it.
            drawHighlightLayer(
                HighlightPaint(l, map, colors, own, stroke, selected?.takeIf { it in own }),
            )
        }

    val inputModifier: Modifier =
        Modifier
            .pointerInput(this) { penLoop() }
            .pointerInput(this) { fingerTapLoop() }

    private fun tester(
        l: TextLayoutResult,
        tolerancePx: Float,
    ) = WordHitTester(LayoutGeometry(l), map.displayed, words, tolerancePx)

    private suspend fun PointerInputScope.penLoop() {
        val slop = TeachHighlight.tapSlop.toPx()
        val tolerance = TeachHighlight.lineBandTolerance.toPx()
        awaitEachGesture {
            var event: PointerEvent
            var down: PointerInputChange?
            do {
                event = awaitPointerEvent(PointerEventPass.Initial)
                down =
                    event.changes.firstOrNull {
                        it.type.isPen() && it.changedToDownIgnoreConsumed() && !host.penWrites
                    }
            } while (down == null)
            target.onPenContact()
            val id = down.id
            val origin = down.position
            val startedAt = down.uptimeMillis
            val erase = penErases(down.type, event.stylusButtonPressed(), host.eraserMode)
            // The pen never scrolls nor selects inside a highlightable block: every event of the gesture is ours.
            down.consume()
            val stroked = StrokeWords(words, map.displayed)
            var stroking = false

            fun feed(p: Offset) {
                val l = layout() ?: return
                stroked.add(tester(l, tolerance).hitWord(p.x, p.y))
            }

            fun publish() {
                val runs =
                    if (erase) {
                        stroked.hitRuns()
                    } else {
                        listOfNotNull(stroked.span())
                    }
                stroke = StrokeUi(erase, runs, host.color)
            }
            try {
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Initial)
                    val c = e.changes.firstOrNull { it.id == id } ?: break
                    c.consume()
                    if (!stroking && (c.position - origin).getDistance() > slop) {
                        stroking = true
                        feed(origin)
                    }
                    if (stroking) {
                        c.historical.forEach { feed(it.position) }
                        feed(c.position)
                        publish()
                    }
                    if (!c.pressed) {
                        // A cancel arrives as an end of the pointer too, with no real UP behind it: nothing to commit.
                        val action = e.motionEvent?.actionMasked
                        if (action == null || action == MotionEvent.ACTION_CANCEL) break
                        if (stroking) {
                            commit(stroked, erase)
                        } else if (c.uptimeMillis - startedAt <= TeachHighlight.tapMaxDuration &&
                            (c.position - origin).getDistance() <= slop
                        ) {
                            tap(c.position, pen = true, tolerance)
                        }
                        break
                    }
                }
            } finally {
                // A cancelled gesture (palm rejection, another window) leaves nothing behind.
                stroke = null
            }
        }
    }

    private fun commit(
        stroked: StrokeWords,
        erase: Boolean,
    ) {
        if (erase) {
            for (run in stroked.eraseRuns()) {
                val source = map.displayToSource(run) ?: continue
                // Widened over marker-only remainders, so the host never ends with an invisible fragment.
                val clean = Highlights.resolveRemove(host.highlights, blockId, source.start, source.end, map)
                host.erase(blockId, clean.start, clean.end)
            }
            return
        }
        val span = stroked.span() ?: return
        val color = host.color
        val sameColor =
            own().filter { it.color == color }.flatMap { map.sourceToDisplay(it.interval) }
        val merged = mergeAcrossBlanks(span, sameColor, map.displayed)
        val source = map.displayToSource(merged) ?: return
        val clean = Highlights.resolveAdd(host.highlights, blockId, source.start, source.end, color, map)
        host.add(blockId, clean.start, clean.end)
    }

    /** A finger tap, in the main pass and never consumed: links and terms keep their own click. */
    private suspend fun PointerInputScope.fingerTapLoop() {
        val slop = TeachHighlight.tapSlop.toPx()
        val tolerance = TeachHighlight.lineBandTolerance.toPx()
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            if (down.type != PointerType.Touch) return@awaitEachGesture
            val up =
                withTimeoutOrNull(TeachHighlight.tapMaxDuration.toLong()) { waitForUpOrCancellation() }
                    ?: return@awaitEachGesture
            if ((up.position - down.position).getDistance() <= slop) tap(up.position, pen = false, tolerance)
        }
    }

    private fun tap(
        at: Offset,
        pen: Boolean,
        tolerancePx: Float,
    ) {
        val l = layout() ?: return
        val t = tester(l, tolerancePx)
        val line = t.lineAt(at.y)
        if (line < 0) return
        val char = t.charAt(line, at.x)
        if (char < 0) return
        val link =
            built.text
                .getLinkAnnotations(char, char + 1)
                .firstOrNull()
                ?.item
        if (link != null) {
            // The pen's down was consumed, so the text never saw a click: run what the click would run.
            if (pen && link is LinkAnnotation.Clickable) link.linkInteractionListener?.onClick(link)
            return
        }
        val source = map.displayToSource(Interval(char, char + 1)) ?: return
        // Same overlap test as the drawing: what is seen can be tapped.
        val hit = Highlights.overlapping(host.highlights, blockId, source) ?: return
        popover =
            PopoverState(hit, at.x, l.getLineTop(line), l.getLineBottom(line), openedAtY = lastY)
    }

    /**
     * [text] with the span background of the inline code under a highlight made transparent, so the highlight fill is
     * what shows behind it (the span background is painted with the text, above anything drawn behind). Text with no
     * highlight on code comes back untouched: it is drawn exactly as before the highlighter existed.
     */
    @Composable
    fun AnnotatedString.withCodeClearedUnderHighlights(): AnnotatedString {
        val runs =
            own()
                .flatMap { map.sourceToDisplay(it.interval) }
                .mapNotNull { run ->
                    val covered =
                        built.codeRanges.any { it.first < run.end && it.last + 1 > run.start }
                    run.takeIf { covered }
                }
        if (runs.isEmpty()) return this
        return remember(this, runs) {
            buildAnnotatedString {
                append(this@withCodeClearedUnderHighlights)
                for (run in runs) addStyle(SpanStyle(background = Color.Transparent), run.start, run.end)
            }
        }
    }

    /** The block moved on the window: a scroll closes the popover (section 8). */
    fun onPositioned(y: Float) {
        lastY = y
        val open = popover ?: return
        if (abs(open.openedAtY - y) > 1f) popover = null
    }

    @Composable
    fun Popover() {
        val state = popover ?: return
        val density = LocalDensity.current
        val gap = with(density) { TeachHighlight.popoverGap.roundToPx() }
        val guard =
            WindowInsets.safeDrawing.getTop(density) + with(density) { TeachTheme.layout.topBarHeight.roundToPx() }
        val provider =
            remember(state, gap, guard) {
                object : PopupPositionProvider {
                    override fun calculatePosition(
                        anchorBounds: IntRect,
                        windowSize: IntSize,
                        layoutDirection: LayoutDirection,
                        popupContentSize: IntSize,
                    ): IntOffset {
                        val x =
                            (anchorBounds.left + state.x.toInt() - popupContentSize.width / 2)
                                .coerceIn(0, maxOf(0, windowSize.width - popupContentSize.width))
                        val above = anchorBounds.top + state.lineTop.toInt() - gap - popupContentSize.height
                        val y =
                            if (above >= guard) {
                                above
                            } else {
                                (anchorBounds.top + state.lineBottom.toInt() + gap)
                                    .coerceAtMost(maxOf(0, windowSize.height - popupContentSize.height))
                            }
                        return IntOffset(x, y)
                    }
                }
            }
        val focus = remember { FocusRequester() }
        val shape = TeachTheme.shapes.md
        Popup(
            popupPositionProvider = provider,
            onDismissRequest = { popover = null },
            properties = PopupProperties(focusable = true),
        ) {
            Box(
                Modifier
                    .raisedShadow(shape)
                    .background(TeachTheme.colors.surfaceRaised, shape)
                    .padding(TeachTheme.spacing.xs),
            ) {
                TeachButton(
                    text = stringResource(R.string.sdui_highlight_remove),
                    onClick = {
                        popover = null
                        host.remove(blockId, state.highlight.start)
                    },
                    modifier = Modifier.widthIn(min = TeachHighlight.popoverButtonMinWidth).focusRequester(focus),
                    variant = ButtonVariant.Text,
                    leadingIcon = TeachIcons.Eraser,
                )
            }
        }
        LaunchedEffect(state) { runCatching { focus.requestFocus() } }
    }

    /**
     * Screen reader node of a highlightable block (section 12): with highlights the text is read with them announced.
     * The modifier is the same element with or without highlights, so a highlight appearing never resets the input.
     */
    @Composable
    fun Modifier.a11y(actions: List<InlineAction>): Modifier {
        val own = own().filter { map.sourceToDisplay(it.interval).isNotEmpty() }
        val notes = rememberNotesA11y(host, blockId)
        val clear = listOfNotNull(notes.clearAction)
        if (own.isEmpty()) {
            val inline = actions.map { a -> CustomAccessibilityAction(a.label) { a.run().let { true } } }
            return semantics {
                notes.stateText?.let { stateDescription = it }
                if (inline.isNotEmpty() || clear.isNotEmpty()) customActions = inline + clear
            }
        }
        val names =
            mapOf(
                HighlightColor.YELLOW to stringResource(R.string.sdui_highlight_yellow),
                HighlightColor.GREEN to stringResource(R.string.sdui_highlight_green),
                HighlightColor.BLUE to stringResource(R.string.sdui_highlight_blue),
                HighlightColor.PINK to stringResource(R.string.sdui_highlight_pink),
            )
        val startTemplate = stringResource(R.string.sdui_highlight_a11y_start)
        val endText = stringResource(R.string.sdui_highlight_a11y_end)
        val removeTemplate = stringResource(R.string.sdui_highlight_remove_action)
        // "2 evidenziazioni, 1 nota a mano": the two counts are one announcement.
        val count =
            listOfNotNull(pluralStringResource(R.plurals.sdui_highlight_count, own.size, own.size), notes.stateText)
                .joinToString(", ")
        val displayed = map.displayed
        val runs =
            own
                .flatMap { h -> map.sourceToDisplay(h.interval).map { it to h } }
                .sortedBy { it.first.start }
        val spoken =
            buildString {
                var at = 0
                for ((run, h) in runs) {
                    if (run.start < at) continue
                    append(displayed, at, run.start)
                    append(String.format(startTemplate, names.getValue(h.color))).append(' ')
                    append(displayed, run.start, run.end)
                    append(endText)
                    at = run.end
                }
                append(displayed, at, displayed.length)
            }
        // One action per highlight, no cap: the screen reader menu is the only non visual way to remove one, and a
        // cap would leave the later ones unremovable until earlier ones are gone (at most 500 per lesson anyway).
        val remove =
            own.map { h ->
                val first = map.sourceToDisplay(h.interval).first()
                val label =
                    displayed
                        .substring(first.start, first.end)
                        .split(Regex("\\s+"))
                        .filter { it.isNotEmpty() }
                        .take(A11Y_WORDS)
                        .joinToString(" ")
                CustomAccessibilityAction(String.format(removeTemplate, label)) {
                    host.remove(blockId, h.start)
                    true
                }
            }
        val inline =
            actions.map { a ->
                CustomAccessibilityAction(a.label) {
                    a.run()
                    true
                }
            }
        return semantics {
            text = AnnotatedString(spoken)
            stateDescription = count
            customActions = inline + remove + clear
        }
    }
}

private fun PointerType.isPen() = this == PointerType.Stylus || this == PointerType.Eraser

/** The side button of a stylus, read from the native event: Compose maps it to no button of its own. */
private fun PointerEvent.stylusButtonPressed(): Boolean =
    motionEvent?.let { it.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY != 0 } == true

@Composable
internal fun rememberHighlightSupport(
    target: HighlightTarget,
    built: BuiltInline,
    layout: () -> TextLayoutResult?,
): HighlightSupport {
    val support = remember(built) { HighlightSupport(built, target) }
    support.target = target
    support.host = LocalHighlightHost.current
    support.layout = layout
    support.colors = TeachTheme.colors
    return support
}
