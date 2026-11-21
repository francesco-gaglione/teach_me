package com.teachme.feature.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachText
import kotlinx.coroutines.launch

private const val MAX_CODE_LINE = 1_500
private val FADE_WIDTH = 24.dp
private val KEY_SCROLL_STEP = 48.dp

/**
 * Fenced code in the chat (chat.md 5.1): 48 dp header with language and copy button, body that scrolls
 * horizontally (no wrapping), focusable with a physical keyboard. [showHeader] false is the table variant
 * (no language, no copy). No syntax highlighting in v1.
 */
@Composable
internal fun ChatCodeBlock(
    code: String,
    language: String?,
    copy: (String) -> Unit,
    modifier: Modifier = Modifier,
    showHeader: Boolean = true,
) {
    val colors = TeachTheme.colors
    val shape = TeachTheme.shapes.sm
    val scroll = rememberScrollState()
    val scope = rememberCoroutineScope()
    val step = with(LocalDensity.current) { KEY_SCROLL_STEP.toPx() }
    var focused by remember { mutableStateOf(false) }
    // A single very long line would exceed the maximum text width: show it in chunks (copy gives the original).
    val shown = remember(code) { code.lines().joinToString("\n") { it.chunked(MAX_CODE_LINE).joinToString("\n") } }
    val description =
        if (language.isNullOrEmpty()) {
            stringResource(R.string.chat_code_description_plain)
        } else {
            stringResource(R.string.chat_code_description, language)
        }
    Column(
        modifier
            .fillMaxWidth()
            .padding(vertical = TeachTheme.spacing.sm)
            .clip(shape)
            .background(colors.codeBg, shape),
    ) {
        if (showHeader) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(
                        min = TeachTheme.layout.minTouchTarget,
                    ).padding(start = TeachTheme.spacing.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                TeachText(
                    language.orEmpty(),
                    Modifier.weight(1f),
                    style = TeachTheme.typography.ui.labelS,
                    color = colors.codeComment,
                )
                CopyButton({ code }, stringResource(R.string.chat_copy_code), copy)
            }
        }
        Box {
            Box(
                Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focused = it.isFocused }
                    .then(if (focused) Modifier.border(TeachTheme.stroke.focus, colors.focus, shape) else Modifier)
                    .onKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                        when (event.key) {
                            Key.DirectionRight -> scope.launch { scroll.scrollBy(step) }
                            Key.DirectionLeft -> scope.launch { scroll.scrollBy(-step) }
                            Key.MoveEnd -> scope.launch { scroll.scrollTo(scroll.maxValue) }
                            Key.MoveHome -> scope.launch { scroll.scrollTo(0) }
                            else -> return@onKeyEvent false
                        }
                        true
                    }.focusable()
                    .horizontalScroll(scroll)
                    .padding(horizontal = TeachTheme.spacing.md, vertical = TeachTheme.spacing.sm),
            ) {
                BasicText(
                    text = shown,
                    modifier = Modifier.semantics { contentDescription = description },
                    style =
                        TeachTheme.typography.code.block
                            .copy(color = colors.codeInk),
                    softWrap = false,
                )
            }
            if (scroll.canScrollForward) {
                Box(
                    Modifier
                        .align(Alignment.CenterEnd)
                        .width(FADE_WIDTH)
                        .fillMaxHeight()
                        .background(Brush.horizontalGradient(listOf(Color.Transparent, colors.codeBg))),
                )
            }
        }
    }
}

@Composable
private fun CodePreviewContent() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        ChatCodeBlock(
            code = "let s = String::from(\"ciao\");\nlet t = s; // s non è più valido, e questa riga è lunga",
            language = "rust",
            copy = {},
        )
        ChatCodeBlock(code = "| a | b |\n|---|---|", language = null, copy = {}, showHeader = false)
    }
}

@Preview(name = "Code light", showBackground = true, widthDp = 360)
@Composable
private fun CodeLightPreview() = TeachTheme(darkTheme = false) { CodePreviewContent() }

@Preview(name = "Code dark", showBackground = true, widthDp = 360)
@Composable
private fun CodeDarkPreview() = TeachTheme(darkTheme = true) { CodePreviewContent() }
