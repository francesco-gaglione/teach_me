package com.teachme.feature.chat.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachText
import com.teachme.feature.chat.ChatCodeBlock

private val LIST_INDENT = 24.dp
private val NUMBER_COLUMN = 28.dp

/**
 * Renders the markdown subset of chat.md section 5. Each block (paragraph, list item, code block) is its own
 * accessibility node so TalkBack moves by paragraph. Parsing is tolerant, so it is safe on a text that is
 * still streaming.
 */
@Composable
internal fun MarkdownText(
    text: String,
    copy: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val blocks = remember(text) { MarkdownParser.parse(text) }
    val colors = TeachTheme.colors
    val reading = TeachTheme.typography.reading
    val strong = reading.bodyStrong.toSpanStyle()
    val code =
        TeachTheme.typography.code.inline.toSpanStyle().copy(
            background = colors.surfaceSunken,
            color = colors.ink,
        )
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.layout.blockGapTight)) {
        blocks.forEach { block ->
            when (block) {
                is MdBlock.Paragraph ->
                    TeachText(
                        remember(
                            block,
                            colors,
                            strong,
                            code,
                        ) { annotate(block.inlines, colors, strong, code, boldAll = false) },
                        style = reading.body,
                        color = colors.ink,
                    )
                is MdBlock.Heading ->
                    Column {
                        Spacer(Modifier.height(TeachTheme.spacing.xxs))
                        TeachText(
                            remember(
                                block,
                                colors,
                                strong,
                                code,
                            ) { annotate(block.inlines, colors, strong, code, boldAll = true) },
                            style = reading.bodyStrong,
                            color = colors.ink,
                        )
                    }
                MdBlock.Rule ->
                    Box(
                        Modifier
                            .padding(vertical = TeachTheme.spacing.sm)
                            .fillMaxWidth()
                            .height(TeachTheme.stroke.hairline)
                            .background(colors.line),
                    )
                is MdBlock.Code ->
                    ChatCodeBlock(block.code, block.language, copy, showHeader = !block.table)
                is MdBlock.ListBlock -> ListBlockView(block, colors, strong, code)
            }
        }
    }
}

@Composable
private fun ListBlockView(
    list: MdBlock.ListBlock,
    colors: TeachColors,
    strong: SpanStyle,
    code: SpanStyle,
) {
    val reading = TeachTheme.typography.reading
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        list.items.forEach { item ->
            Row(
                Modifier.fillMaxWidth().padding(start = LIST_INDENT * item.level).semantics(mergeDescendants = true) {},
            ) {
                if (item.ordinal != null) {
                    TeachText(
                        "${item.ordinal}.",
                        Modifier.width(NUMBER_COLUMN).padding(end = TeachTheme.spacing.xxs),
                        style = reading.bodyStrong,
                        color = colors.inkSoft,
                        textAlign = TextAlign.End,
                    )
                } else {
                    TeachText("•", Modifier.width(LIST_INDENT), style = reading.body, color = colors.inkSoft)
                }
                Spacer(Modifier.width(if (item.ordinal != null) TeachTheme.spacing.xxs else 0.dp))
                TeachText(
                    remember(
                        item,
                        colors,
                        strong,
                        code,
                    ) { annotate(item.inlines, colors, strong, code, boldAll = false) },
                    Modifier.weight(1f),
                    style = reading.body,
                    color = colors.ink,
                )
            }
        }
    }
}

private fun annotate(
    inlines: List<MdInline>,
    colors: TeachColors,
    strong: SpanStyle,
    code: SpanStyle,
    boldAll: Boolean,
): AnnotatedString =
    buildAnnotatedString {
        fun styleOf(
            bold: Boolean,
            italic: Boolean,
        ): SpanStyle {
            var style = SpanStyle()
            if (bold || boldAll) style = style.merge(strong)
            if (italic) style = style.merge(SpanStyle(fontStyle = FontStyle.Italic))
            return style
        }
        inlines.forEach { inline ->
            when (inline) {
                is MdInline.Text ->
                    withStyle(
                        if (inline.code) {
                            styleOf(
                                inline.bold,
                                inline.italic,
                            ).merge(code)
                        } else {
                            styleOf(inline.bold, inline.italic)
                        },
                    ) {
                        append(inline.text)
                    }
                is MdInline.Link -> {
                    val linkStyle =
                        styleOf(inline.bold, inline.italic).merge(
                            SpanStyle(color = colors.accent, textDecoration = TextDecoration.Underline),
                        )
                    withLink(LinkAnnotation.Url(inline.url, TextLinkStyles(linkStyle))) { append(inline.label) }
                }
                MdInline.LineBreak -> append('\n')
            }
        }
    }

@Composable
private fun MarkdownPreviewContent() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        MarkdownText(
            "Il **borrow checker** controlla *ogni* riferimento con `&mut`.\n\n" +
                "1. Primo passo\n2. Secondo passo\n   - sotto\n\n" +
                "Vedi [la guida](https://doc.rust-lang.org/book/).\n\n" +
                "```rust\nlet s = String::from(\"ciao\");\n```\n\n## Titolo\n\n---\n\n> citazione",
            copy = {},
        )
    }
}

@Preview(name = "Markdown light", showBackground = true, widthDp = 360)
@Composable
private fun MarkdownLightPreview() = TeachTheme(darkTheme = false) { MarkdownPreviewContent() }

@Preview(name = "Markdown dark", showBackground = true, widthDp = 360)
@Composable
private fun MarkdownDarkPreview() = TeachTheme(darkTheme = true) { MarkdownPreviewContent() }
