package com.teachme.sdui.render

import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.sdui.R
import com.teachme.sdui.model.ExerciseCode
import kotlinx.coroutines.launch

private const val KEY_SCROLL_STEP_DP = 48
private const val ACTION_SCROLL_STEP_DP = 160
private const val LEFT_FADE_MIN_COLUMNS = 8
private const val MONO_ADVANCE_EM = 0.6f
private val FADE_WIDTH = 24.dp
private val FADE_HEIGHT = 32.dp
private val MIN_HEIGHT = 48.dp
private val UNLABELLED_LANGUAGES = setOf("text", "output")

/** True when the snippet gets the small language label (docs/design/components/_exercises.md section 10.1). */
internal fun showsLanguageLabel(language: String): Boolean = language.isNotBlank() && language !in UNLABELLED_LANGUAGES

/**
 * The code an exercise asks to read (`_exercises.md` section 10): the `code` block reduced to the snippet itself, so
 * no header, copy button, line numbers or caption. Horizontal scroll only, collapsed past 16 lines like `code.md`.
 * It sits above the question and is a single TalkBack node.
 */
@Composable
internal fun ExerciseCodeView(
    code: ExerciseCode,
    expandKey: String,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val lines = remember(code.code) { normalizeCode(code.code) }
    val collapsedLines =
        TeachTheme.layout.codeCollapsedLines.value
            .toInt()
    var expanded by remember(expandKey) { mutableStateOf(false) }
    val collapsible = lines.size > collapsedLines
    val shown = if (collapsible && !expanded) lines.take(collapsedLines) else lines
    val annotated = remember(shown, code.language, colors) { highlightedCode(shown, code.language, colors) }
    val codeStyle =
        TeachTheme.typography.code.block
            .copy(color = colors.codeInk)
    val scroll = rememberScrollState()
    val scope = rememberCoroutineScope()
    val interaction = rememberInteractionState()
    val density = LocalDensity.current
    val linesText = pluralStringResource(R.plurals.sdui_code_lines, lines.size, lines.size)
    val description =
        if (showsLanguageLabel(code.language)) {
            stringResource(R.string.sdui_code_a11y, code.language, linesText)
        } else {
            stringResource(R.string.sdui_code_plain_a11y, linesText)
        }
    val scrollRightLabel = stringResource(R.string.sdui_code_scroll_right)
    val scrollLeftLabel = stringResource(R.string.sdui_code_scroll_left)
    val stepPx = with(density) { ACTION_SCROLL_STEP_DP.dp.toPx() }
    val leftFadeMinPx = with(density) { codeStyle.fontSize.toPx() } * MONO_ADVANCE_EM * LEFT_FADE_MIN_COLUMNS

    Column(modifier.fillMaxWidth()) {
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(min = MIN_HEIGHT)
                .clip(TeachTheme.shapes.sm)
                .background(colors.codeBg)
                .padding(top = TeachTheme.spacing.sm, bottom = TeachTheme.spacing.sm),
        ) {
            if (showsLanguageLabel(code.language)) {
                // Not part of the selection and not read: the node below says the language.
                TeachText(
                    text = code.language,
                    modifier =
                        Modifier
                            .padding(start = TeachTheme.spacing.md, bottom = TeachTheme.spacing.xxs)
                            .clearAndSetSemantics { },
                    style = TeachTheme.typography.ui.labelS,
                    color = colors.codeComment,
                )
            }
            Box(
                Modifier
                    .fillMaxWidth()
                    .clearAndSetSemantics {
                        contentDescription = description
                        text = AnnotatedString(lines.joinToString("\n"))
                        if (scroll.maxValue > 0) {
                            customActions =
                                listOf(
                                    CustomAccessibilityAction(scrollRightLabel) {
                                        scope.launch { scroll.animateScrollBy(stepPx) }
                                        scroll.canScrollForward
                                    },
                                    CustomAccessibilityAction(scrollLeftLabel) {
                                        scope.launch { scroll.animateScrollBy(-stepPx) }
                                        scroll.canScrollBackward
                                    },
                                )
                        }
                    },
            ) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .focusRing(interaction.focused, TeachTheme.shapes.sm)
                        .focusable(interactionSource = interaction.source)
                        .onKeyEvent { event ->
                            if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                            when (event.key) {
                                Key.DirectionRight, Key.DirectionLeft -> {
                                    val dir = if (event.key == Key.DirectionRight) 1 else -1
                                    scope.launch {
                                        scroll.scrollBy(
                                            dir * with(density) { KEY_SCROLL_STEP_DP.dp.toPx() },
                                        )
                                    }
                                    true
                                }
                                Key.MoveHome -> {
                                    scope.launch { scroll.scrollTo(0) }
                                    true
                                }
                                Key.MoveEnd -> {
                                    scope.launch { scroll.scrollTo(scroll.maxValue) }
                                    true
                                }
                                else -> false
                            }
                        }.drawWithContent {
                            drawContent()
                            val w = FADE_WIDTH.toPx()
                            if (scroll.canScrollForward) {
                                drawRect(
                                    Brush.horizontalGradient(
                                        listOf(Color.Transparent, colors.codeBg),
                                        startX = size.width - w,
                                        endX = size.width,
                                    ),
                                    Offset(size.width - w, 0f),
                                    Size(w, size.height),
                                )
                            }
                            // The left fade only matters when there is a lot more than a couple of columns behind.
                            if (scroll.value > 0 && scroll.maxValue > leftFadeMinPx) {
                                drawRect(
                                    Brush.horizontalGradient(
                                        listOf(colors.codeBg, Color.Transparent),
                                        startX = 0f,
                                        endX = w,
                                    ),
                                    Offset(0f, 0f),
                                    Size(w, size.height),
                                )
                            }
                        }.horizontalScroll(scroll),
                ) {
                    SelectionContainer {
                        BasicText(
                            text = annotated,
                            modifier = Modifier.padding(horizontal = TeachTheme.spacing.md),
                            style = codeStyle,
                            softWrap = false,
                        )
                    }
                }
                if (collapsible && !expanded) {
                    Box(
                        Modifier
                            .align(Alignment.BottomCenter)
                            .fillMaxWidth()
                            .height(FADE_HEIGHT)
                            .background(Brush.verticalGradient(listOf(Color.Transparent, colors.codeBg))),
                    )
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
    }
}

/** The code of an exercise, or nothing, followed by the 4 dp that bring the card gap from 12 to 16 (section 10.1). */
@Composable
internal fun ColumnScope.ExerciseCodeSlot(
    code: ExerciseCode?,
    expandKey: String,
) {
    if (code == null) return
    ExerciseCodeView(code, expandKey, Modifier.padding(bottom = TeachTheme.spacing.xxs))
}
