package com.teachme.feature.lesson

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.key
import androidx.compose.ui.res.stringResource
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachBottomSheet
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.model.GlossaryTerm
import com.teachme.sdui.logic.SummaryBuilder
import com.teachme.sdui.render.GlossarySheet
import com.teachme.sdui.render.SduiHost
import com.teachme.sdui.render.SourceSheet

/** Sheet currently on screen, kept as a string so it survives rotation. */
internal object SheetCode {
    const val NONE = ""
    const val INDEX = "index"

    fun term(key: String) = "term:$key"

    fun source(key: String) = "source:$key"
}

/** The glossary and source sheets, the index sheet of a narrow window and the glossary sheet opened from the summary. */
@Composable
internal fun LessonSheets(
    ready: LessonUiState.Ready,
    sheet: String,
    onSheet: (String) -> Unit,
    topicTerm: GlossaryTerm?,
    onTopicTerm: (GlossaryTerm?) -> Unit,
    items: List<LessonItem>,
    scrollToKey: (String) -> Unit,
    host: SduiHost,
    vm: LessonViewModel,
    onOpenLesson: (String) -> Unit,
) {
    topicTerm?.let { term ->
        val current = ready.lesson
        val origin =
            term.lessonId?.let { id ->
                if (current != null &&
                    id == current.id
                ) {
                    LessonOrigin(id, current.number, current.title)
                } else {
                    LessonOrigin(id, null, null)
                }
            }
        TopicGlossarySheet(
            term = term,
            origin = origin,
            numberOf = { id -> current?.takeIf { it.id == id }?.number },
            onOpenLesson =
                origin?.takeIf { it.lessonId != current?.id }?.let {
                    { id ->
                        onTopicTerm(null)
                        onOpenLesson(id)
                    }
                },
            onDismiss = { onTopicTerm(null) },
        )
    }
    when {
        sheet == SheetCode.INDEX ->
            TeachBottomSheet(
                stringResource(R.string.lesson_index_button),
                onDismissRequest = { onSheet(SheetCode.NONE) },
            ) {
                LessonContextContent(
                    ready = ready,
                    onOpenSection = { index ->
                        onSheet(SheetCode.NONE)
                        firstKey(items, index)?.let(scrollToKey)
                    },
                    onOpenTerm = host::openTerm,
                    onOpenSource = host::openSource,
                    onDisclosure = vm::setDisclosure,
                )
                TeachButton(stringResource(R.string.lesson_close_sheet), {
                    onSheet(SheetCode.NONE)
                }, Modifier.fillMaxWidth(), variant = ButtonVariant.Secondary)
            }
        sheet.startsWith("term:") ->
            ready.document.glossaryEntry(sheet.removePrefix("term:"))?.let {
                GlossarySheet(it) {
                    onSheet(SheetCode.NONE)
                }
            }
        sheet.startsWith("source:") -> {
            val key = sheet.removePrefix("source:")
            val source = ready.document.source(key)
            val number = ready.document.sourceNumber(key)
            if (source != null && number != null) {
                SourceSheet(number, source, onOpen = host::openUrl, onDismiss = { onSheet(SheetCode.NONE) })
            }
        }
    }
}

/** The closing screen: what the learner did, the backend job that follows and what can be done next. */
@Composable
internal fun LessonSummaryPane(
    ready: LessonUiState.Ready,
    title: String,
    vm: LessonViewModel,
    onClose: () -> Unit,
    onAsk: (String) -> Unit,
    onReview: (String) -> Unit,
    onOpenTerm: (GlossaryTerm) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        LessonTopBar(title, null, 1f, showIndexButton = false, onClose = onClose, onIndex = {})
        val summary =
            remember(ready.sections, ready.saved) {
                SummaryBuilder.build(
                    ready.sections,
                    { ready.exercise(it) },
                ) { ready.document.glossaryEntry(it)?.term }
            }
        val backToTopic = {
            vm.requestLessonsTab()
            onClose()
        }
        val askDone = stringResource(R.string.lesson_summary_ask_done, title)
        val askEmpty = stringResource(R.string.lesson_summary_ask_empty, title)
        LessonSummaryView(
            objective = ready.document.objective,
            pdfTitle =
                ready.document.sources
                    .firstOrNull { it.isUserPdf }
                    ?.title,
            pdfMore = (ready.document.sources.count { it.isUserPdf } - 1).coerceAtLeast(0),
            summary = summary,
            completion = ready.completion,
            closedElsewhere = ready.closedElsewhere,
            onBackToTopic = backToTopic,
            onAskTutor = { empty -> onAsk(if (empty) askEmpty else askDone) },
            onReview = {
                onReview(blockKey(it))
                vm.dismissSummary()
            },
            onReread = vm::dismissSummary,
            onOpenTerm = onOpenTerm,
            onSeeRegister = {
                vm.requestProgressTab()
                onClose()
            },
            onRetrySend = vm::retryCompletion,
            onRetryJob = vm::retryCompletionJob,
        )
    }
}

/** The pen palette (tool, colours, width, eraser, undo/redo); the eraser switch is announced to TalkBack. */
@Composable
internal fun LessonPaletteBar(
    ready: LessonUiState.Ready,
    palette: HighlightPaletteState,
    compact: Boolean,
    vm: LessonViewModel,
    announcer: PoliteAnnouncer,
    onUndo: () -> Unit,
    onRedo: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val eraserOnSpoken = stringResource(R.string.lesson_eraser_on_spoken)
    val eraserOffSpoken = stringResource(R.string.lesson_eraser_off_spoken)
    LessonPalette(
        visible = palette.visible,
        compact = compact,
        tool = ready.penTool,
        onTool = vm::setPenTool,
        highlightColor = ready.highlightColor,
        onHighlightColor = vm::setHighlightColor,
        inkColor = ready.inkColor,
        onInkColor = vm::setInkColor,
        inkWidth = ready.inkWidth,
        onInkWidth = vm::setInkWidth,
        canUndo = ready.canUndoInk,
        canRedo = ready.canRedoInk,
        onUndo = onUndo,
        onRedo = onRedo,
        eraserOn = ready.eraserMode,
        onEraser = { on ->
            vm.setEraserMode(on)
            announcer.say(if (on) eraserOnSpoken else eraserOffSpoken)
        },
        onTouched = palette::onPaletteTouched,
        modifier = modifier,
    )
}
