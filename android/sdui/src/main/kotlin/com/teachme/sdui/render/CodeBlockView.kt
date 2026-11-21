package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.CodeBlock
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val GUTTER_MIN_LINES = 3
private const val KEY_SCROLL_STEP_DP = 48
private const val COPIED_FEEDBACK_MS = 1_500L
private val FADE_WIDTH = 24.dp
private val FADE_HEIGHT = 32.dp
private val GUTTER_WIDTH = 32.dp
private val HEADER_HEIGHT = 48.dp

internal fun normalizeCode(code: String): List<String> =
    code
        .replace("\r\n", "\n")
        .replace('\r', '\n')
        .replace("\t", "    ")
        .trimEnd('\n')
        .split('\n')

internal fun highlightedCode(
    lines: List<String>,
    language: String,
    colors: TeachColors,
): AnnotatedString {
    val text = lines.joinToString("\n")
    val tokens = SyntaxHighlighter.tokenize(text, language)
    return buildAnnotatedString {
        var last = 0
        for (t in tokens) {
            if (t.start > last) append(text.substring(last, t.start))
            val color =
                when (t.kind) {
                    TokenKind.Keyword -> colors.codeKeyword
                    TokenKind.Str -> colors.codeString
                    TokenKind.Comment -> colors.codeComment
                    TokenKind.Number -> colors.codeNumber
                    TokenKind.Function -> colors.codeFunction
                    TokenKind.Type -> colors.codeType
                    TokenKind.Punctuation -> colors.codePunctuation
                }
            withStyle(SpanStyle(color = color)) { append(text.substring(t.start, t.end)) }
            last = t.end
        }
        if (last < text.length) append(text.substring(last))
    }
}

/** `code` (docs/design/components/code.md): line numbers from 3 lines, highlight, collapse at 16, copy. */
@Composable
internal fun CodeBlockView(block: CodeBlock) {
    val env = LocalSdui.current
    val colors = TeachTheme.colors
    val lines = remember(block.code) { normalizeCode(block.code) }
    val collapsedLines =
        TeachTheme.layout.codeCollapsedLines.value
            .toInt()
    var expanded by remember(block.id) { mutableStateOf(false) }
    val collapsible = lines.size > collapsedLines
    val shown = if (collapsible && !expanded) lines.take(collapsedLines) else lines
    val annotated = remember(shown, block.language, colors) { highlightedCode(shown, block.language, colors) }
    val showGutter = lines.size >= GUTTER_MIN_LINES
    val highlight =
        remember(block.highlightLines, shown.size) { block.highlightLines.filter { it in 1..shown.size }.toSet() }
    val codeStyle =
        TeachTheme.typography.code.block
            .copy(color = colors.codeInk)
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    val scroll = rememberScrollState()
    val scope = rememberCoroutineScope()
    val interaction = rememberInteractionState()
    val density = androidx.compose.ui.platform.LocalDensity.current
    val topPadding = TeachTheme.spacing.sm
    val language = block.language.takeIf { it.isNotBlank() && it != "text" }
    val linesText = pluralStringResource(R.plurals.sdui_code_lines, lines.size, lines.size)
    val description = stringResource(R.string.sdui_code_a11y, block.language, linesText)
    val copyLabel = stringResource(R.string.sdui_code_copy)
    val copiedLabel = stringResource(R.string.sdui_code_copied)
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            delay(COPIED_FEEDBACK_MS)
            copied = false
        }
    }
    val copy = {
        env.host.copyText(lines.joinToString("\n"))
        copied = true
    }

    Column(Modifier.fillMaxWidth().semantics { isTraversalGroup = true }) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(TeachTheme.shapes.sm)
                .background(colors.codeBg),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = HEADER_HEIGHT)
                    .padding(start = TeachTheme.spacing.md)
                    // The copy button is read after the code (traversal order), not before it.
                    .semantics {
                        isTraversalGroup = true
                        traversalIndex = 1f
                    },
                verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
            ) {
                TeachText(
                    text = language.orEmpty(),
                    modifier = Modifier.weight(1f),
                    style = TeachTheme.typography.ui.labelS,
                    color = colors.codeComment,
                )
                TeachIconButton(
                    icon = if (copied) TeachIcons.Check else TeachIcons.Copy,
                    contentDescription = if (copied) copiedLabel else copyLabel,
                    onClick = copy,
                    modifier = Modifier.semantics { if (copied) liveRegion = LiveRegionMode.Polite },
                    tint = colors.codeComment,
                )
            }
            Box(Modifier.fillMaxWidth().height(TeachTheme.stroke.hairline).background(colors.codeLine))
            SelectionContainer {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .semantics {
                            isTraversalGroup = true
                            traversalIndex = 0f
                            contentDescription = description
                            text = AnnotatedString(lines.joinToString("\n"))
                            customActions =
                                listOf(
                                    CustomAccessibilityAction(copyLabel) {
                                        copy()
                                        true
                                    },
                                )
                        }.drawBehind {
                            val l = layout ?: return@drawBehind
                            val top = topPadding.toPx()
                            for (line in highlight) {
                                val index = line - 1
                                if (index >= l.lineCount) continue
                                val y = top + l.getLineTop(index)
                                drawRect(
                                    colors.codeLineHighlight,
                                    Offset(0f, y),
                                    Size(
                                        size.width,
                                        l.getLineBottom(index) - l.getLineTop(index),
                                    ),
                                )
                            }
                        },
                ) {
                    Row(Modifier.padding(vertical = TeachTheme.spacing.sm)) {
                        if (showGutter) {
                            BasicText(
                                text = (1..shown.size).joinToString("\n"),
                                modifier =
                                    Modifier
                                        .width(GUTTER_WIDTH + TeachTheme.spacing.md)
                                        .padding(end = TeachTheme.spacing.sm)
                                        .clearAndSetSemantics { },
                                style = codeStyle.copy(color = colors.codeComment, textAlign = TextAlign.End),
                            )
                        } else {
                            Box(Modifier.width(TeachTheme.spacing.md))
                        }
                        Box(
                            Modifier
                                .weight(1f)
                                .focusRing(interaction.focused, TeachTheme.shapes.sm)
                                .focusable(interactionSource = interaction.source)
                                .onKeyEvent { event ->
                                    val dir =
                                        when (event.key) {
                                            Key.DirectionRight -> 1
                                            Key.DirectionLeft -> -1
                                            else -> 0
                                        }
                                    if (dir != 0 && event.type == KeyEventType.KeyDown) {
                                        scope.launch {
                                            scroll.scrollBy(
                                                dir * with(density) { KEY_SCROLL_STEP_DP.dp.toPx() },
                                            )
                                        }
                                        true
                                    } else if (event.type == KeyEventType.KeyDown && event.key == Key.MoveHome) {
                                        scope.launch { scroll.scrollTo(0) }
                                        true
                                    } else if (event.type == KeyEventType.KeyDown && event.key == Key.MoveEnd) {
                                        scope.launch { scroll.scrollTo(scroll.maxValue) }
                                        true
                                    } else {
                                        false
                                    }
                                }.drawWithContent {
                                    drawContent()
                                    if (scroll.canScrollForward) {
                                        val w = FADE_WIDTH.toPx()
                                        drawRect(
                                            Brush.horizontalGradient(
                                                listOf(Color.Transparent, colors.codeBg),
                                                startX =
                                                    size.width - w,
                                                endX = size.width,
                                            ),
                                            Offset(size.width - w, 0f),
                                            Size(w, size.height),
                                        )
                                    }
                                }.horizontalScroll(scroll),
                        ) {
                            BasicText(
                                text = annotated,
                                modifier = Modifier.padding(end = TeachTheme.spacing.md),
                                style = codeStyle,
                                softWrap = false,
                                onTextLayout = { layout = it },
                            )
                        }
                    }
                    if (collapsible && !expanded) {
                        Box(
                            Modifier
                                .align(androidx.compose.ui.Alignment.BottomCenter)
                                .fillMaxWidth()
                                .height(FADE_HEIGHT)
                                .background(Brush.verticalGradient(listOf(Color.Transparent, colors.codeBg))),
                        )
                    }
                }
            }
        }
        if (collapsible) {
            TeachButton(
                text =
                    if (expanded) {
                        stringResource(R.string.sdui_code_show_less)
                    } else {
                        pluralStringResource(
                            R.plurals.sdui_code_show_more,
                            lines.size - collapsedLines,
                            lines.size - collapsedLines,
                        )
                    },
                onClick = { expanded = !expanded },
                variant = ButtonVariant.Text,
            )
        }
        block.caption?.takeIf { it.isNotBlank() }?.let {
            Box(Modifier.padding(top = TeachTheme.spacing.xs)) {
                InlineText(
                    markdown = it,
                    style = TeachTheme.typography.ui.body,
                    color = colors.inkSoft,
                    lang = block.lang,
                )
            }
        }
    }
}
