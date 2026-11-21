package com.teachme.sdui.render

import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.Dp
import com.teachme.core.designsystem.TeachTheme
import com.teachme.sdui.model.AskAgent
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.Callout
import com.teachme.sdui.model.Checklist
import com.teachme.sdui.model.CodeBlock
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.Exercise
import com.teachme.sdui.model.FillBlank
import com.teachme.sdui.model.Flashcard
import com.teachme.sdui.model.GlossaryTermBlock
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.LessonLink
import com.teachme.sdui.model.MissingComponent
import com.teachme.sdui.model.OpenAnswer
import com.teachme.sdui.model.OrderSteps
import com.teachme.sdui.model.Paragraph
import com.teachme.sdui.model.PrimarySource
import com.teachme.sdui.model.QuizMulti
import com.teachme.sdui.model.QuizSingle

typealias BlockRenderer = @Composable (Block) -> Unit

/**
 * Maps the wire `type` of a block to the composable that draws it. A type that is not registered (or a
 * block of the wrong class for its type) is drawn as the neutral unavailable row: the app never crashes
 * on content it does not know.
 */
class BlockRegistry(
    private val renderers: Map<String, BlockRenderer>,
) {
    val types: Set<String> get() = renderers.keys

    @Composable
    fun Render(block: Block) {
        val renderer = renderers[block.type]
        when {
            renderer == null -> UnavailableBlockView()
            block is Exercise -> ReadOnlyIfClosed(block) { renderer(block) }
            else -> renderer(block)
        }
    }
}

private inline fun <reified T : Block> typed(crossinline draw: @Composable (T) -> Unit): BlockRenderer =
    { block -> if (block is T) draw(block) else UnavailableBlockView() }

/** The catalog v1: seventeen types. `missing_component` draws its fallback blocks in sequence, boxless. */
val DefaultBlockRegistry: BlockRegistry =
    BlockRegistry(
        mapOf(
            "heading" to typed<Heading> { HeadingBlock(it) },
            "paragraph" to typed<Paragraph> { ParagraphBlock(it) },
            "callout" to typed<Callout> { CalloutBlock(it) },
            "code" to typed<CodeBlock> { CodeBlockView(it) },
            "glossary_term" to typed<GlossaryTermBlock> { GlossaryTermBlockView(it) },
            "quiz_single" to typed<QuizSingle> { QuizSingleBlock(it) },
            "quiz_multi" to typed<QuizMulti> { QuizMultiBlock(it) },
            "fill_blank" to typed<FillBlank> { FillBlankBlock(it) },
            "order_steps" to typed<OrderSteps> { OrderStepsBlock(it) },
            "flashcard" to typed<Flashcard> { FlashcardBlock(it) },
            "open_answer" to typed<OpenAnswer> { OpenAnswerBlock(it) },
            "checklist" to typed<Checklist> { ChecklistBlock(it) },
            "primary_source" to typed<PrimarySource> { PrimarySourceBlock(it) },
            "lesson_link" to typed<LessonLink> { LessonLinkBlock(it) },
            "ask_agent" to typed<AskAgent> { AskAgentBlock(it) },
            "diagram" to typed<Diagram> { DiagramBlockView(it) },
            "missing_component" to
                typed<MissingComponent> { missing ->
                    // Only reached outside the lesson column (which flattens it): same result, as a plain column.
                    androidx.compose.foundation.layout.Column(
                        verticalArrangement =
                            androidx.compose.foundation.layout.Arrangement.spacedBy(
                                TeachTheme.layout.blockGap,
                            ),
                    ) {
                        missing.fallback.forEach { DefaultBlockRegistry.Render(it) }
                    }
                },
        ),
    )

/** Vertical rhythm between consecutive blocks (docs/design/components/README.md). */
enum class BlockGap { None, Small, Paragraph, Heading, Block, H3, Section }

/** The gap before [next] when [previous] is drawn right above it. */
fun gapBetween(
    previous: Block?,
    next: Block,
    titleBlockId: String? = null,
): BlockGap =
    when {
        previous == null -> BlockGap.None
        previous is Heading -> BlockGap.Heading
        next is Heading && next.level == 3 -> BlockGap.H3
        next is Heading && (next.level == 2 || next.level == 1 && next.id != titleBlockId) -> BlockGap.Section
        previous is Paragraph && next is Paragraph -> BlockGap.Paragraph
        previous is LessonLink && next is LessonLink -> BlockGap.Small
        else -> BlockGap.Block
    }

@Composable
fun BlockGap.dp(): Dp =
    when (this) {
        BlockGap.None -> TeachTheme.spacing.none
        BlockGap.Small -> TeachTheme.spacing.xs
        BlockGap.Paragraph -> TeachTheme.spacing.md
        BlockGap.Heading -> TeachTheme.layout.blockGapTight
        BlockGap.Block -> TeachTheme.layout.blockGap
        BlockGap.H3 -> TeachTheme.spacing.xl
        BlockGap.Section -> TeachTheme.spacing.xxl
    }
