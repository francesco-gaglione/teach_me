package com.teachme.feature.lesson

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.intl.Locale
import androidx.compose.ui.text.toLowerCase
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.raisedShadow
import com.teachme.sdui.highlight.HighlightColor
import com.teachme.sdui.ink.InkColor
import com.teachme.sdui.ink.InkWidth
import kotlinx.coroutines.delay
import kotlin.math.max

private const val PRESSED_SCALE = 0.92f
private val SWATCH_RING_GAP = 2.dp
private val CHEVRON_SIZE = 12.dp

/**
 * The one palette of the lesson screen (highlight.md section 6, handwriting.md section 3): the tool selector, then what
 * the active tool needs. The highlighter shows its four colours; the pen shows the four inks, the three widths and
 * Undo/Redo, or, when [compact] (window under 660 dp), a single "pen style" button opening a popover with inks and widths. Both
 * tools end with the Eraser switch (handwriting.md section 3.4); the highlighter's colours collapse into a popover too.
 * Fades in and out by itself.
 */
@Composable
fun LessonPalette(
    visible: Boolean,
    compact: Boolean,
    tool: PenTool,
    onTool: (PenTool) -> Unit,
    highlightColor: HighlightColor,
    onHighlightColor: (HighlightColor) -> Unit,
    inkColor: InkColor,
    onInkColor: (InkColor) -> Unit,
    inkWidth: InkWidth,
    onInkWidth: (InkWidth) -> Unit,
    canUndo: Boolean,
    canRedo: Boolean,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    eraserOn: Boolean,
    onEraser: (Boolean) -> Unit,
    onTouched: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduced = TeachTheme.reducedMotion
    val motion = TeachTheme.motion
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = fadeIn(tween(if (reduced) 0 else motion.fastMs)),
        exit = fadeOut(tween(if (reduced) 0 else motion.slowMs)),
    ) {
        val colors = TeachTheme.colors
        val toolGroup = stringResource(R.string.lesson_palette_tool)
        Row(
            Modifier
                .raisedShadow(TeachTheme.shapes.full)
                .background(colors.surfaceRaised, TeachTheme.shapes.full)
                .height(TeachHighlight.paletteHeight)
                .padding(horizontal = TeachHighlight.palettePaddingX),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(Modifier.selectableGroup().semantics { contentDescription = toolGroup }) {
                for (entry in PenTool.entries) {
                    ToolButton(entry, entry == tool, onClick = {
                        onTouched()
                        onTool(entry)
                    })
                }
            }
            Separator()
            // The elements that change dissolve in `fast`; with "reduce animations" the change is instant.
            Crossfade(tool, animationSpec = tween(if (reduced) 0 else motion.fastMs), label = "palette-tool") { shown ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    val eraser: @Composable () -> Unit = {
                        EraserButton(eraserOn, onToggle = {
                            onTouched()
                            onEraser(it)
                        })
                    }
                    val pickHighlight = { color: HighlightColor ->
                        onTouched()
                        onHighlightColor(color)
                    }
                    when {
                        shown == PenTool.HIGHLIGHTER && compact -> {
                            CompactHighlighter(highlightColor, pickHighlight, onTouched)
                            Separator()
                            eraser()
                        }
                        shown == PenTool.HIGHLIGHTER -> {
                            HighlightColors(highlightColor, onSelect = pickHighlight)
                            Separator()
                            eraser()
                        }
                        compact -> {
                            CompactPen(inkColor, inkWidth, onInkColor, onInkWidth, onTouched)
                            eraser()
                            UndoRedo(canUndo, canRedo, onUndo, onRedo, onTouched)
                        }
                        else -> {
                            InkColors(inkColor, onSelect = {
                                onTouched()
                                onInkColor(it)
                            })
                            Separator()
                            InkWidths(inkWidth, onSelect = {
                                onTouched()
                                onInkWidth(it)
                            })
                            Separator()
                            eraser()
                            UndoRedo(canUndo, canRedo, onUndo, onRedo, onTouched)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Separator() {
    Box(
        Modifier
            .padding(horizontal = TeachHandwriting.paletteSeparatorGap)
            .width(TeachHandwriting.paletteSeparatorWidth)
            .fillMaxHeight(SEPARATOR_FRACTION)
            .background(TeachTheme.colors.line),
    )
}

private const val SEPARATOR_FRACTION = 0.5f

// --- the tool selector ----------------------------------------------------------------------------------------------

@Composable
private fun ToolButton(
    tool: PenTool,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    val source = remember { MutableInteractionSource() }
    val scale = rememberPressScale(source)
    val focused by source.collectIsFocusedAsState()
    val name =
        stringResource(if (tool == PenTool.PEN) R.string.lesson_tool_pen else R.string.lesson_tool_highlighter)
    val chosen = stringResource(R.string.lesson_highlight_selected)
    Box(
        Modifier
            .size(TeachHandwriting.paletteToolButton)
            .focusRing(focused, CircleShape)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }.clip(CircleShape)
            .background(if (selected) colors.accentSoft else colors.surfaceRaised)
            .selectable(
                selected = selected,
                interactionSource = source,
                indication = null,
                role = Role.RadioButton,
                onClick = onClick,
            ).semantics {
                contentDescription = name
                if (selected) stateDescription = chosen
            },
        contentAlignment = Alignment.Center,
    ) {
        TeachIcon(
            if (tool == PenTool.PEN) TeachIcons.PenLine else TeachIcons.Highlighter,
            contentDescription = null,
            tint = if (selected) colors.accent else colors.inkSoft,
            size = TeachHandwriting.paletteToolIcon,
        )
        if (selected) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = SWATCH_RING_GAP)
                    .width(TeachHandwriting.paletteSelectedBarWidth)
                    .height(TeachHandwriting.paletteSelectedBarHeight)
                    .background(colors.accent, TeachTheme.shapes.full),
            )
        }
    }
}

// --- the colours and widths ---------------------------------------------------------------------------------------

@Composable
private fun HighlightColors(
    selected: HighlightColor,
    onSelect: (HighlightColor) -> Unit,
) {
    val colors = TeachTheme.colors
    val description = stringResource(R.string.lesson_highlight_palette)
    Row(Modifier.selectableGroup().semantics { contentDescription = description }) {
        for (color in HighlightColor.entries) {
            Swatch(color.label(), color == selected, { onSelect(color) }) {
                HighlightSample(color, selected == color, colors)
            }
        }
    }
}

@Composable
private fun HighlightSample(
    color: HighlightColor,
    checked: Boolean,
    colors: TeachColors,
) {
    Box(
        Modifier
            .size(TeachHighlight.paletteSwatch)
            .background(color.fill(colors), CircleShape)
            .border(TeachHighlight.paletteSwatchBorder, color.line(colors), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (checked) CheckMark(colors.ink)
    }
}

@Composable
private fun InkColors(
    selected: InkColor,
    onSelect: (InkColor) -> Unit,
) {
    val colors = TeachTheme.colors
    val description = stringResource(R.string.lesson_ink_color_group)
    Row(Modifier.selectableGroup().semantics { contentDescription = description }) {
        for (color in InkColor.entries) {
            Swatch(
                color.label(),
                color == selected,
                { onSelect(color) },
            ) { InkSwatch(color, color == selected, colors) }
        }
    }
}

@Composable
private fun InkSwatch(
    color: InkColor,
    selected: Boolean,
    colors: TeachColors,
) {
    Box(
        Modifier.size(TeachHighlight.paletteSwatch).background(color.paint(colors), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        if (selected) CheckMark(colors.penOn)
    }
}

@Composable
private fun InkWidths(
    selected: InkWidth,
    onSelect: (InkWidth) -> Unit,
) {
    val colors = TeachTheme.colors
    val description = stringResource(R.string.lesson_ink_width_group)
    Row(Modifier.selectableGroup().semantics { contentDescription = description }) {
        for (width in InkWidth.entries) {
            Swatch(width.label(), width == selected, { onSelect(width) }) { WidthSample(width, colors) }
        }
    }
}

@Composable
private fun WidthSample(
    width: InkWidth,
    colors: TeachColors,
) {
    Box(
        Modifier.size(TeachHandwriting.paletteWidthSwatch).background(colors.surfaceSunken, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .width(TeachHandwriting.paletteWidthSampleLength)
                .height(width.penDp().dp)
                .background(colors.ink, TeachTheme.shapes.full),
        )
    }
}

@Composable
private fun BoxScope.CheckMark(tint: androidx.compose.ui.graphics.Color) {
    TeachIcon(
        TeachIcons.Check,
        contentDescription = null,
        tint = tint,
        size = TeachHighlight.paletteCheckIcon,
        modifier = Modifier.align(Alignment.Center),
    )
}

/** A 48 dp target holding a 36 dp sample, with the ring around the chosen one (the choice is not only a colour). */
@Composable
private fun Swatch(
    name: String,
    selected: Boolean,
    onClick: () -> Unit,
    sample: @Composable () -> Unit,
) {
    val colors = TeachTheme.colors
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val scale = rememberPressScale(source)
    val chosen = stringResource(R.string.lesson_highlight_selected)
    Box(
        Modifier
            .size(TeachHighlight.paletteSwatchTarget)
            .focusRing(focused, CircleShape)
            .selectable(
                selected = selected,
                interactionSource = source,
                indication = null,
                role = Role.RadioButton,
                onClick = onClick,
            ).semantics {
                contentDescription = name
                if (selected) stateDescription = chosen
            }.graphicsLayer {
                scaleX = scale
                scaleY = scale
            },
        contentAlignment = Alignment.Center,
    ) {
        if (selected) {
            Box(
                Modifier
                    .size(TeachHighlight.paletteSwatch + SWATCH_RING_GAP * 2 + TeachHighlight.paletteSwatchBorder * 2)
                    .border(TeachHighlight.paletteSwatchBorder, colors.ink, CircleShape),
            )
        }
        sample()
    }
}

@Composable
private fun rememberPressScale(source: MutableInteractionSource): Float {
    val pressed by source.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) PRESSED_SCALE else 1f,
        animationSpec = tween(if (TeachTheme.reducedMotion) 0 else TeachTheme.motion.fastMs),
        label = "palette-press",
    )
    return scale
}

// --- the eraser switch --------------------------------------------------------------------------------------------

/** The pen tip as an eraser (handwriting.md section 3.4): a two-state switch, off unless the reader turns it on. */
@Composable
private fun EraserButton(
    on: Boolean,
    onToggle: (Boolean) -> Unit,
) {
    val colors = TeachTheme.colors
    val source = remember { MutableInteractionSource() }
    val scale = rememberPressScale(source)
    val focused by source.collectIsFocusedAsState()
    val name = stringResource(R.string.lesson_eraser)
    val state = stringResource(if (on) R.string.lesson_eraser_on else R.string.lesson_eraser_off)
    Box(
        Modifier
            .size(TeachHandwriting.paletteToolButton)
            .focusRing(focused, CircleShape)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }.clip(CircleShape)
            .background(if (on) colors.accentSoft else colors.surfaceRaised)
            .toggleable(
                value = on,
                interactionSource = source,
                indication = null,
                role = Role.Switch,
                onValueChange = onToggle,
            ).semantics {
                contentDescription = name
                stateDescription = state
            },
        contentAlignment = Alignment.Center,
    ) {
        TeachIcon(
            TeachIcons.Eraser,
            contentDescription = null,
            tint = if (on) colors.accent else colors.inkSoft,
            size = TeachHandwriting.paletteToolIcon,
        )
        if (on) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = SWATCH_RING_GAP)
                    .width(TeachHandwriting.paletteSelectedBarWidth)
                    .height(TeachHandwriting.paletteSelectedBarHeight)
                    .background(colors.accent, TeachTheme.shapes.full),
            )
        }
    }
}

// --- undo and redo ------------------------------------------------------------------------------------------------

@Composable
private fun UndoRedo(
    canUndo: Boolean,
    canRedo: Boolean,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    onTouched: () -> Unit,
) {
    val ink = TeachTheme.colors.ink
    TeachIconButton(
        TeachIcons.Undo,
        stringResource(R.string.lesson_ink_undo),
        {
            onTouched()
            onUndo()
        },
        Modifier.size(TeachHandwriting.paletteUndoRedoButton),
        enabled = canUndo,
        tint = ink,
    )
    TeachIconButton(
        TeachIcons.Redo,
        stringResource(R.string.lesson_ink_redo),
        {
            onTouched()
            onRedo()
        },
        Modifier.size(TeachHandwriting.paletteUndoRedoButton),
        enabled = canRedo,
        tint = ink,
    )
}

// --- the compact forms --------------------------------------------------------------------------------------------

/** One "pen style" button: the current ink and width, a popover with a row of inks and a row of widths (3.2). */
@Composable
private fun CompactPen(
    inkColor: InkColor,
    inkWidth: InkWidth,
    onInkColor: (InkColor) -> Unit,
    onInkWidth: (InkWidth) -> Unit,
    onTouched: () -> Unit,
) {
    val colors = TeachTheme.colors
    var open by remember { mutableStateOf(false) }
    // Every touch inside the popover restarts its idle time; a choice does not close it.
    var touches by remember { mutableIntStateOf(0) }
    val name = stringResource(R.string.lesson_ink_style)
    val state =
        stringResource(R.string.lesson_ink_style_state, inkColor.label(), inkWidth.label().toLowerCase(Locale.current))
    Chip(name, state, open, onClick = {
        onTouched()
        open = !open
    }, popover = {
        Popover(onClose = { open = false }, idleKey = touches) {
            Column {
                InkColors(inkColor, onSelect = {
                    onTouched()
                    touches++
                    onInkColor(it)
                })
                InkWidths(inkWidth, onSelect = {
                    onTouched()
                    touches++
                    onInkWidth(it)
                })
            }
        }
    }) {
        Box(
            Modifier.size(TeachHighlight.paletteSwatch).background(inkColor.paint(colors), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                Modifier
                    .width(TeachHandwriting.paletteWidthSampleLength)
                    .height(inkWidth.penDp().dp)
                    .background(colors.penOn, TeachTheme.shapes.full),
            )
        }
    }
}

/** The highlighter's colour as one button with a popover of the four colours; choosing one closes it. */
@Composable
private fun CompactHighlighter(
    selected: HighlightColor,
    onSelect: (HighlightColor) -> Unit,
    onTouched: () -> Unit,
) {
    val colors = TeachTheme.colors
    var open by remember { mutableStateOf(false) }
    val name = stringResource(R.string.lesson_highlight_palette)
    Chip(name, selected.label(), open, onClick = {
        onTouched()
        open = !open
    }, popover = {
        // Nothing to restart the idle time with: choosing a colour closes the popover.
        Popover(onClose = { open = false }, idleKey = 0) {
            HighlightColors(selected, onSelect = {
                onSelect(it)
                open = false
            })
        }
    }) { HighlightSample(selected, false, colors) }
}

/** The current sample in the pill with a small `chevron-up`, so it shows that it opens (handwriting.md 3.2). */
@Composable
private fun Chip(
    name: String,
    state: String?,
    expanded: Boolean,
    onClick: () -> Unit,
    popover: @Composable () -> Unit,
    sample: @Composable () -> Unit,
) {
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val scale = rememberPressScale(source)
    Box(
        Modifier
            .size(TeachHandwriting.paletteToolButton)
            .focusRing(focused, CircleShape)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }.clickable(interactionSource = source, indication = null, role = Role.Button, onClick = onClick)
            .semantics {
                contentDescription = name
                if (state != null) stateDescription = state
            },
        contentAlignment = Alignment.Center,
    ) {
        sample()
        TeachIcon(
            TeachIcons.ChevronUp,
            contentDescription = null,
            tint = TeachTheme.colors.inkMuted,
            size = CHEVRON_SIZE,
            modifier = Modifier.align(Alignment.BottomEnd),
        )
        if (expanded) popover()
    }
}

/** Above the anchor, [gap] away, inside the window. */
private class AboveAnchor(
    private val gap: Int,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val x =
            (anchorBounds.center.x - popupContentSize.width / 2).coerceIn(
                0,
                max(
                    0,
                    windowSize.width - popupContentSize.width,
                ),
            )
        val y = (anchorBounds.top - gap - popupContentSize.height).coerceAtLeast(0)
        return IntOffset(x, y)
    }
}

/** The `Popover` of the design system on top of the pill; closes after the idle time when nothing was chosen. */
@Composable
private fun Popover(
    onClose: () -> Unit,
    idleKey: Int,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current
    val gap =
        with(density) {
            (
                (TeachHighlight.paletteHeight - TeachHandwriting.paletteToolButton) / 2 +
                    TeachHandwriting.palettePopoverGap
            ).roundToPx()
        }
    LaunchedEffect(idleKey) {
        delay(TeachHighlight.paletteIdleHideMs.toLong())
        onClose()
    }
    val shape = TeachTheme.shapes.md
    Popup(
        popupPositionProvider = remember(gap) { AboveAnchor(gap) },
        onDismissRequest = onClose,
        properties = PopupProperties(focusable = true),
    ) {
        Box(
            Modifier
                .raisedShadow(shape)
                .background(TeachTheme.colors.surfaceRaised, shape)
                .padding(TeachTheme.spacing.xxs),
        ) { content() }
    }
}

// --- names and colours --------------------------------------------------------------------------------------------

private fun HighlightColor.fill(colors: TeachColors) =
    when (this) {
        HighlightColor.YELLOW -> colors.highlightYellow
        HighlightColor.GREEN -> colors.highlightGreen
        HighlightColor.BLUE -> colors.highlightBlue
        HighlightColor.PINK -> colors.highlightPink
    }

private fun HighlightColor.line(colors: TeachColors) =
    when (this) {
        HighlightColor.YELLOW -> colors.highlightYellowLine
        HighlightColor.GREEN -> colors.highlightGreenLine
        HighlightColor.BLUE -> colors.highlightBlueLine
        HighlightColor.PINK -> colors.highlightPinkLine
    }

@Composable
private fun HighlightColor.label() =
    stringResource(
        when (this) {
            HighlightColor.YELLOW -> R.string.lesson_highlight_yellow
            HighlightColor.GREEN -> R.string.lesson_highlight_green
            HighlightColor.BLUE -> R.string.lesson_highlight_blue
            HighlightColor.PINK -> R.string.lesson_highlight_pink
        },
    )

@Composable
private fun InkColor.label() =
    stringResource(
        when (this) {
            InkColor.INK -> R.string.lesson_ink_ink
            InkColor.BLUE -> R.string.lesson_ink_blue
            InkColor.RED -> R.string.lesson_ink_red
            InkColor.GREEN -> R.string.lesson_ink_green
        },
    )

@Composable
private fun InkWidth.label() =
    stringResource(
        when (this) {
            InkWidth.THIN -> R.string.lesson_ink_thin
            InkWidth.MEDIUM -> R.string.lesson_ink_medium
            InkWidth.THICK -> R.string.lesson_ink_thick
        },
    )

// --- previews -----------------------------------------------------------------------------------------------------

@Composable
private fun PaletteSample(
    tool: PenTool,
    compact: Boolean,
    canUndo: Boolean = true,
    ink: InkColor = InkColor.BLUE,
    eraserOn: Boolean = false,
) {
    Box(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        LessonPalette(
            visible = true,
            compact = compact,
            tool = tool,
            onTool = {},
            highlightColor = HighlightColor.YELLOW,
            onHighlightColor = {},
            inkColor = ink,
            onInkColor = {},
            inkWidth = InkWidth.MEDIUM,
            onInkWidth = {},
            canUndo = canUndo,
            canRedo = false,
            onUndo = {},
            onRedo = {},
            eraserOn = eraserOn,
            onEraser = {},
            onTouched = {},
        )
    }
}

@androidx.compose.ui.tooling.preview.Preview(name = "Highlighter light", showBackground = true)
@Composable
private fun HighlighterLightPreview() = TeachTheme(darkTheme = false) { PaletteSample(PenTool.HIGHLIGHTER, false) }

@androidx.compose.ui.tooling.preview.Preview(name = "Pen expanded light", showBackground = true)
@Composable
private fun PenExpandedLightPreview() = TeachTheme(darkTheme = false) { PaletteSample(PenTool.PEN, false) }

@androidx.compose.ui.tooling.preview.Preview(name = "Pen expanded dark, empty undo", showBackground = true)
@Composable
private fun PenExpandedDarkPreview() =
    TeachTheme(darkTheme = true) { PaletteSample(PenTool.PEN, false, canUndo = false, ink = InkColor.INK) }

@androidx.compose.ui.tooling.preview.Preview(name = "Pen compact light", showBackground = true)
@Composable
private fun PenCompactLightPreview() = TeachTheme(darkTheme = false) { PaletteSample(PenTool.PEN, true) }

@androidx.compose.ui.tooling.preview.Preview(name = "Pen compact dark", showBackground = true)
@Composable
private fun PenCompactDarkPreview() =
    TeachTheme(darkTheme = true) {
        PaletteSample(PenTool.PEN, true, ink = InkColor.RED)
    }

@androidx.compose.ui.tooling.preview.Preview(name = "Pen expanded eraser on", showBackground = true)
@Composable
private fun PenEraserOnPreview() = TeachTheme(darkTheme = false) { PaletteSample(PenTool.PEN, false, eraserOn = true) }

@androidx.compose.ui.tooling.preview.Preview(name = "Highlighter compact light", showBackground = true)
@Composable
private fun HighlighterCompactLightPreview() =
    TeachTheme(darkTheme = false) {
        PaletteSample(PenTool.HIGHLIGHTER, true)
    }

@androidx.compose.ui.tooling.preview.Preview(name = "Highlighter compact dark, eraser on", showBackground = true)
@Composable
private fun HighlighterCompactDarkPreview() =
    TeachTheme(darkTheme = true) { PaletteSample(PenTool.HIGHLIGHTER, true, eraserOn = true) }
