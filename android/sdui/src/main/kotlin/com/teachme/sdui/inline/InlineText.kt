package com.teachme.sdui.inline

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.teachme.core.designsystem.TeachColors
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.R
import com.teachme.sdui.highlight.HighlightTarget
import com.teachme.sdui.highlight.LocalHighlightHost
import com.teachme.sdui.highlight.NoHighlights
import com.teachme.sdui.highlight.SourceMap
import com.teachme.sdui.highlight.SourceMapBuilder
import com.teachme.sdui.highlight.rememberHighlightSupport
import com.teachme.sdui.render.LocalSdui
import com.teachme.sdui.render.SduiEnvironment
import com.teachme.sdui.render.effectiveLang
import com.teachme.sdui.render.hyphensFor
import com.teachme.sdui.render.localesFor

/** An interactive span, also offered to TalkBack as a custom action of the whole text. */
internal data class InlineAction(
    val label: String,
    val run: () -> Unit,
)

internal class BuiltInline(
    val text: AnnotatedString,
    /** Character ranges of glossary terms, for the dashed underline. */
    val termRanges: List<IntRange>,
    val actions: List<InlineAction>,
    /** Source markdown to displayed text, for the highlighter; null when the text is not highlightable. */
    val sourceMap: SourceMap?,
    /** Character ranges of inline code, so a highlight can take over their background. */
    val codeRanges: List<IntRange> = emptyList(),
)

private const val CITATION_SIZE_SP = 12
private const val CODE_SIZE_DELTA_SP = 2
private const val THIN_SPACE = '\u2009'
private const val HAIR_SPACE = '\u200A'
private const val NBSP = '\u00A0'
private const val ZERO_WIDTH_SPACE = '\u200B'

/** Inline code up to this many characters never breaks across lines (docs/design/components/_exercises.md 10.10). */
internal const val INLINE_CODE_UNBREAKABLE_MAX = 24

/**
 * What the app draws for an inline code span: padded by a hair space on each side (so the background does not touch
 * the neighbouring letters), and either unbreakable (<= 24 chars: spaces become NBSP, so the whole span moves to the
 * next line instead of splitting its background) or breakable only after `(`, `::` and `.` (a zero width space there,
 * never between two letters). The spec names a word joiner for the break points, but U+2060 forbids a break and
 * U+200B allows it, which is what the rule describes.
 */
internal fun inlineCodeDisplay(text: String): String {
    val body =
        if (text.length <= INLINE_CODE_UNBREAKABLE_MAX) {
            text.replace(' ', NBSP)
        } else {
            buildString {
                for ((i, c) in text.withIndex()) {
                    append(c)
                    val breakAfter = c == '(' || c == '.' || (c == ':' && i > 0 && text[i - 1] == ':')
                    if (breakAfter && i + 1 < text.length) append(ZERO_WIDTH_SPACE)
                }
            }
        }
    return "$HAIR_SPACE$body$HAIR_SPACE"
}

internal fun buildInline(
    source: String,
    base: TextStyle,
    colors: TeachColors,
    codeStyle: TextStyle,
    env: SduiEnvironment,
    interactive: Boolean,
    labels: InlineLabels,
    withSourceMap: Boolean = false,
): BuiltInline {
    val termRanges = mutableListOf<IntRange>()
    val codeRanges = mutableListOf<IntRange>()
    val actions = mutableListOf<InlineAction>()
    // The map (and the per-char source offsets behind it) is only built for highlightable text.
    val mapBuilder = if (withSourceMap) SourceMapBuilder(source) else null
    val text =
        buildAnnotatedString {
            appendSpanned(
                if (withSourceMap) {
                    InlineMarkdown.parseSpanned(source)
                } else {
                    InlineMarkdown.parse(source).map { SpannedInline(it, 0, 0) }
                },
                base,
                colors,
                codeStyle,
                env,
                interactive,
                labels,
                termRanges,
                actions,
                mapBuilder,
                codeRanges,
            )
        }
    return BuiltInline(text, termRanges, actions, mapBuilder?.build(text.text), codeRanges)
}

/** Appends [nodes] to this builder. Term ranges are offsets in the builder, so chunks can be chained. */
internal fun AnnotatedString.Builder.appendInline(
    nodes: List<Inline>,
    base: TextStyle,
    colors: TeachColors,
    codeStyle: TextStyle,
    env: SduiEnvironment,
    interactive: Boolean,
    labels: InlineLabels,
    termRanges: MutableList<IntRange>,
    actions: MutableList<InlineAction>,
) = appendSpanned(
    nodes.map { SpannedInline(it, 0, 0) },
    base,
    colors,
    codeStyle,
    env,
    interactive,
    labels,
    termRanges,
    actions,
    null,
    null,
)

private fun AnnotatedString.Builder.appendSpanned(
    spans: List<SpannedInline>,
    base: TextStyle,
    colors: TeachColors,
    codeStyle: TextStyle,
    env: SduiEnvironment,
    interactive: Boolean,
    labels: InlineLabels,
    termRanges: MutableList<IntRange>,
    actions: MutableList<InlineAction>,
    map: SourceMapBuilder?,
    codeRanges: MutableList<IntRange>?,
) {
    val codeSize: TextUnit = (base.fontSize.value - CODE_SIZE_DELTA_SP).sp
    run {
        run {
            for (span in spans) {
                val node = span.node
                val before = length
                val emphasis =
                    SpanStyle(
                        fontWeight = if (node.bold) FontWeight.SemiBold else null,
                        fontStyle = if (node.italic) FontStyle.Italic else null,
                    )
                when (node) {
                    is Inline.Text -> withStyle(emphasis) { append(node.text) }
                    is Inline.Code -> {
                        val shown = inlineCodeDisplay(node.text)
                        codeRanges?.add(length until length + shown.length)
                        withStyle(
                            emphasis.merge(
                                SpanStyle(
                                    fontFamily = codeStyle.fontFamily,
                                    fontSize = codeSize,
                                    background = colors.surfaceSunken,
                                    color = colors.ink,
                                    fontFeatureSettings = codeStyle.fontFeatureSettings,
                                ),
                            ),
                        ) { append(shown) }
                    }
                    is Inline.Link ->
                        if (interactive) {
                            val link =
                                LinkAnnotation.Clickable(
                                    tag = "link:${node.url}",
                                    styles =
                                        TextLinkStyles(
                                            style =
                                                emphasis.merge(
                                                    SpanStyle(
                                                        color = colors.accent,
                                                        textDecoration = TextDecoration.Underline,
                                                    ),
                                                ),
                                        ),
                                ) { env.host.openUrl(node.url) }
                            withLink(link) { append(node.label) }
                        } else {
                            withStyle(emphasis) { append(node.label) }
                        }
                    is Inline.Term -> {
                        val entry = env.document.glossaryEntry(node.key)
                        val shown = node.text ?: entry?.term ?: node.key
                        if (interactive && entry != null) {
                            val start = length
                            val link =
                                LinkAnnotation.Clickable(
                                    tag = "term:${node.key}",
                                    styles = TextLinkStyles(style = emphasis),
                                ) { env.host.openTerm(node.key) }
                            withLink(link) { append(shown) }
                            termRanges += start until length
                            actions += InlineAction(labels.term(shown)) { env.host.openTerm(node.key) }
                        } else {
                            withStyle(emphasis) { append(shown) }
                        }
                    }
                    is Inline.Cite -> {
                        val number = env.document.sourceNumber(node.key)
                        val source = env.document.source(node.key)
                        if (number != null && source != null) {
                            val style =
                                emphasis.merge(
                                    SpanStyle(
                                        color = colors.knowledge,
                                        fontSize = CITATION_SIZE_SP.sp,
                                        fontWeight = FontWeight.SemiBold,
                                    ),
                                )
                            append(THIN_SPACE)
                            if (interactive) {
                                val link =
                                    LinkAnnotation.Clickable(
                                        tag = "cite:${node.key}",
                                        styles = TextLinkStyles(style = style),
                                    ) { env.host.openSource(node.key) }
                                withLink(link) { append("[$number]") }
                                actions +=
                                    InlineAction(
                                        labels.cite(
                                            number,
                                            if (source.isUserPdf) labels.pdfTitle(source.title) else source.title,
                                        ),
                                    ) {
                                        env.host.openSource(node.key)
                                    }
                            } else {
                                withStyle(style) { append("[$number]") }
                            }
                        }
                    }
                }
                map?.append(span, length - before)
            }
        }
    }
}

internal class InlineLabels(
    val term: (String) -> String,
    val cite: (Int, String) -> String,
    /** The title of a source that comes from the learner's PDF, as a screen reader says it ("…, dal tuo PDF"). */
    val pdfTitle: (String) -> String = { it },
)

/**
 * Text with the inline markdown of the contract (`docs/design/components/paragraph.md`). [interactive]
 * false turns terms, citations and links into plain text (quiz options, steps, flashcard fronts...).
 * Terms get the dashed `knowledge` underline: the shape is the non color cue.
 */
@Composable
fun InlineText(
    markdown: String,
    style: TextStyle,
    modifier: Modifier = Modifier,
    color: Color = TeachTheme.colors.ink,
    interactive: Boolean = true,
    lang: String? = null,
    maxLines: Int = Int.MAX_VALUE,
    /** Non null for the text of a `paragraph` or `callout`: the learner can highlight it (highlight.md). */
    highlight: HighlightTarget? = null,
) {
    val env = LocalSdui.current
    val colors = TeachTheme.colors
    val codeStyle = TeachTheme.typography.code.inline
    val termLabel = stringResource(R.string.sdui_term_action)
    val citeLabel = stringResource(R.string.sdui_cite_action)
    val pdfTitleLabel = stringResource(R.string.sdui_pdf_title_a11y)
    val labels =
        remember(termLabel, citeLabel, pdfTitleLabel) {
            InlineLabels(
                term = { String.format(termLabel, it) },
                cite = { n, title -> String.format(citeLabel, n, title) },
                pdfTitle = { String.format(pdfTitleLabel, it) },
            )
        }
    // Outside a lesson screen there is no highlighter: the text behaves exactly as plain text (pen included).
    val highlighting = highlight != null && LocalHighlightHost.current !== NoHighlights
    val built =
        remember(markdown, style, colors, env, interactive, labels, highlighting) {
            buildInline(
                markdown,
                style,
                colors,
                codeStyle,
                env,
                interactive,
                labels,
                withSourceMap = highlighting,
            )
        }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    val support = highlight?.takeIf { highlighting }?.let { rememberHighlightSupport(it, built) { layout } }
    // Under a highlight the highlight is what shows behind inline code, not the span background (section 3.3).
    val shownText = support?.let { with(it) { built.text.withCodeClearedUnderHighlights() } } ?: built.text
    val knowledge = colors.knowledge
    val textLang = effectiveLang(lang, env.contentLanguage)
    val resolved =
        style.copy(
            color = color,
            lineBreak = LineBreak.Paragraph,
            hyphens = hyphensFor(textLang, env.contentLanguage),
            localeList = localesFor(textLang),
        )
    val text: @Composable (Modifier) -> Unit = { textModifier ->
        BasicText(
            text = shownText,
            modifier =
                textModifier
                    .fillMaxWidth()
                    .then(support?.drawModifier ?: Modifier)
                    .drawBehind {
                        val l = layout ?: return@drawBehind
                        val dash = PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 3.dp.toPx()))
                        for (range in built.termRanges) {
                            if (range.last >= l.layoutInput.text.length) continue
                            val firstLine = l.getLineForOffset(range.first)
                            val lastLine = l.getLineForOffset(range.last)
                            for (line in firstLine..lastLine) {
                                val x0 =
                                    if (line ==
                                        firstLine
                                    ) {
                                        l.getHorizontalPosition(range.first, true)
                                    } else {
                                        l.getLineLeft(line)
                                    }
                                val x1 =
                                    if (line ==
                                        lastLine
                                    ) {
                                        l.getHorizontalPosition(range.last + 1, true)
                                    } else {
                                        l.getLineRight(line)
                                    }
                                val y = l.getLineBaseline(line) + 3.dp.toPx()
                                drawLine(
                                    knowledge,
                                    Offset(x0, y),
                                    Offset(x1, y),
                                    strokeWidth = 1.5.dp.toPx(),
                                    pathEffect = dash,
                                )
                            }
                        }
                    }.then(support?.inputModifier ?: Modifier)
                    .then(
                        when {
                            support != null -> with(support) { Modifier.a11y(built.actions) }
                            built.actions.isEmpty() -> Modifier
                            else ->
                                Modifier.semantics {
                                    customActions =
                                        built.actions.map { a ->
                                            CustomAccessibilityAction(a.label) {
                                                a.run()
                                                true
                                            }
                                        }
                                }
                        },
                    ),
            style = resolved,
            maxLines = maxLines,
            overflow = if (maxLines == Int.MAX_VALUE) TextOverflow.Clip else TextOverflow.Ellipsis,
            onTextLayout = { layout = it },
        )
    }
    if (support == null) {
        text(modifier)
    } else {
        Box(modifier.onGloballyPositioned { support.onPositioned(it.positionInWindow().y) }) {
            text(Modifier)
            support.Popover()
        }
    }
}
