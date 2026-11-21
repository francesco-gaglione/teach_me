package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R
import com.teachme.sdui.diagram.planDiagram
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.DiagramValidation
import com.teachme.sdui.model.validateDiagram

/**
 * `diagram` (docs/design/components/diagram.md). Entry point: the model is validated here and the result
 * decides between drawing the figure and the text fallback. Never throws, whatever the document contains.
 */
@Composable
internal fun DiagramBlockView(block: Diagram) {
    val validation = remember(block) { validateDiagram(block) }
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        when (validation) {
            is DiagramValidation.Drawable -> DiagramFigure(validation)
            is DiagramValidation.TextOnly -> DiagramTextFallback(block)
        }
        DiagramCaption(block)
    }
}

/**
 * Draws the three layouts (flow, tree, cycle) for a validated diagram. The orientation is chosen by measuring the
 * real text in the width of the column; when neither orientation fits, the text fallback is shown instead
 * (diagram.md "Regola comune"). Never scrolls horizontally and never scales the text.
 */
@Composable
internal fun DiagramFigure(drawable: DiagramValidation.Drawable) {
    val diagram = drawable.diagram
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    val labelStyle =
        TeachTheme.typography.ui.label
            .forLang(diagram.lang)
    val detailStyle =
        TeachTheme.typography.ui.bodyS
            .forLang(diagram.lang)
    val framePadding = TeachTheme.spacing.md
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val drawingWidth = (maxWidth - framePadding * 2).value
        val plan =
            remember(drawable, drawingWidth, density, measurer, labelStyle, detailStyle) {
                val measure = DiagramTextMeasure(diagram, measurer, density, labelStyle, detailStyle, detailStyle)
                planDiagram(diagram, drawable.layout, drawingWidth, density.fontScale, measure)
            }
        if (plan == null) {
            DiagramTextFallback(diagram)
        } else {
            DiagramFrame(diagram) { DiagramDrawing(diagram, plan) }
        }
    }
}

/**
 * The framed figure area. Drawn figure: one single TalkBack node, an image described by "<title>. <alt>".
 * Text fallback ([drawn] false): one node with the same text and no image role (it is read as text).
 */
@Composable
internal fun DiagramFrame(
    diagram: Diagram,
    drawn: Boolean = true,
    content: @Composable () -> Unit,
) {
    val title = diagram.title?.takeIf { it.isNotBlank() }
    val description = if (title == null) diagram.alt else stringResource(R.string.sdui_diagram_a11y, title, diagram.alt)
    val frame =
        Modifier
            .fillMaxWidth()
            .background(TeachTheme.colors.surfaceSunken, TeachTheme.shapes.md)
            .padding(TeachTheme.spacing.md)
    val semantic =
        if (drawn) {
            frame.semantics(mergeDescendants = false) {
                role = Role.Image
                contentDescription = description
            }
        } else {
            val spoken = AnnotatedString(description).forLang(diagram.lang)
            frame.clearAndSetSemantics { text = spoken }
        }
    Column(modifier = semantic, verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md)) {
        // The drawn content is not read by TalkBack: `alt` is the only description of the figure.
        Column(
            modifier = if (drawn) Modifier.clearAndSetSemantics { } else Modifier,
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            if (title != null) {
                TeachText(
                    title,
                    modifier = Modifier.fillMaxWidth(),
                    style =
                        TeachTheme.typography.ui.labelL
                            .forLang(diagram.lang),
                    color = TeachTheme.colors.ink,
                )
            }
            content()
        }
    }
}

/** The degraded but complete version of the figure: `alt` in words, with a label saying so (not an error). */
@Composable
internal fun DiagramTextFallback(diagram: Diagram) {
    DiagramFrame(diagram, drawn = false) {
        SelectionContainer {
            Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TeachIcon(
                        TeachIcons.ImageOff,
                        contentDescription = null,
                        tint = TeachTheme.colors.inkMuted,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                    TeachText(
                        stringResource(R.string.sdui_diagram_text_label),
                        style = TeachTheme.typography.ui.labelS,
                        color = TeachTheme.colors.inkMuted,
                    )
                }
                TeachText(
                    diagram.alt,
                    modifier = Modifier.fillMaxWidth(),
                    style =
                        TeachTheme.typography.reading.body
                            .forLang(diagram.lang),
                    color = TeachTheme.colors.ink,
                )
            }
        }
    }
}

/** The caption sits outside the frame, as a separate selectable text node (an empty one is not drawn). */
@Composable
private fun DiagramCaption(diagram: Diagram) {
    if (diagram.caption.isBlank()) return
    SelectionContainer {
        InlineText(markdown = diagram.caption, style = TeachTheme.typography.ui.bodyL, lang = diagram.lang)
    }
}

@Composable
internal fun TextStyle.forLang(lang: String?): TextStyle =
    copy(localeList = localesFor(effectiveLang(lang, LocalSdui.current.contentLanguage)))

@Composable
private fun AnnotatedString.forLang(lang: String?): AnnotatedString {
    val locales = localesFor(effectiveLang(lang, LocalSdui.current.contentLanguage))
    return buildAnnotatedString {
        append(this@forLang)
        addStyle(SpanStyle(localeList = locales), 0, length)
    }
}
