package com.teachme.feature.lesson

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachHighlight
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachScreen
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.screenMargin
import com.teachme.sdui.render.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val SLOW_LOADING_MS = 8_000L

/**
 * The lesson screen (docs/design/screens/lesson.md). [onAskAgent] opens the Tutor chat with the composer
 * prefilled; null means "not wired yet" and the screen says so instead of doing nothing.
 */
@Composable
fun LessonRoute(
    onClose: () -> Unit,
    onAskAgent: ((LessonAskRequest) -> Unit)?,
    modifier: Modifier = Modifier,
    onOpenLesson: (String) -> Unit = {},
    viewModel: LessonViewModel = hiltViewModel(),
    audioViewModel: LessonAudioViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val audioState by audioViewModel.state.collectAsStateWithLifecycle()
    val audioControls = remember(audioState, audioViewModel) { LessonAudioControls(audioState, audioViewModel::toggle) }
    // Reading stops when the app goes to the background and when the screen leaves (audio.md section 6).
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) {
        audioViewModel.stop()
        audioViewModel.onStopped()
    }
    LifecycleEventEffect(Lifecycle.Event.ON_START) { audioViewModel.onStarted() }
    DisposableEffect(audioViewModel) { onDispose { audioViewModel.stop() } }
    val toast = remember { TeachToastState() }
    val palette = remember { HighlightPaletteState() }
    palette.Run()
    val toastScope = rememberCoroutineScope()
    val close = {
        viewModel.flushState()
        onClose()
    }
    BackHandler(onBack = close)
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.flushState() }
    // The highlights leave now, without waiting for the debounce, when the app is paused and when the screen is
    // disposed (closing and leaving inside the app never pause the activity). A refused set is not sent again by
    // these: only a new edit, opening the lesson and "Riprova ora" do. highlight.md section 11.
    LifecycleEventEffect(Lifecycle.Event.ON_PAUSE) {
        viewModel.syncHighlightsNow()
        viewModel.syncInkNow()
    }
    // Back in the foreground: send what is queued, grade pending answers once, follow the closing job.
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { viewModel.onResume() }
    val tutorUnavailable = stringResource(R.string.lesson_tutor_unavailable)
    val ask: (String) -> Unit =
        remember(onAskAgent, tutorUnavailable, viewModel) {
            { prompt ->
                val request = viewModel.askRequest(prompt)
                if (onAskAgent != null && request != null) {
                    onAskAgent(request)
                } else {
                    toastScope.launch { toast.show(tutorUnavailable) }
                }
            }
        }
    // While the palette is on screen the toasts make room for it (highlight.md section 6).
    val toastLift = if (palette.visible) TeachHighlight.paletteToastLift else 0.dp
    TeachScreen(
        modifier = modifier,
        toast = { TeachToastHost(toast, Modifier.padding(bottom = toastLift)) },
    ) {
        when (val s = state) {
            LessonUiState.Loading -> LoadingContent(close)
            LessonUiState.LoadFailed ->
                MessageContent(
                    close,
                    StatusVariant.Danger,
                    R.string.lesson_load_failed_title,
                    R.string.lesson_load_failed_body,
                    R.string.lesson_retry,
                    viewModel::retryLoad,
                    null,
                )
            LessonUiState.Gone ->
                MessageContent(
                    close,
                    StatusVariant.Info,
                    R.string.lesson_gone_title,
                    null,
                    R.string.lesson_back_to_topic,
                    close,
                    null,
                )
            LessonUiState.Unusable ->
                MessageContent(
                    close,
                    StatusVariant.Info,
                    R.string.lesson_unusable_title,
                    null,
                    R.string.lesson_back_to_topic,
                    close,
                    { ask("") },
                )
            is LessonUiState.Ready ->
                CompositionLocalProvider(LocalLessonAudio provides audioControls) {
                    LessonContent(s, viewModel, audioViewModel, toast, palette, close, ask, onOpenLesson)
                }
        }
    }
}

@Composable
private fun LoadingContent(onClose: () -> Unit) {
    var slow by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SLOW_LOADING_MS)
        slow = true
    }
    Column(Modifier.fillMaxSize()) {
        LessonTopBar("", null, 0f, showIndexButton = false, onClose = onClose, onIndex = {})
        Column(
            Modifier.padding(screenMargin()),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            TeachStatusPanel(
                StatusVariant.Progress,
                stringResource(if (slow) R.string.lesson_loading_slow else R.string.lesson_loading),
            )
        }
    }
}

@Composable
private fun MessageContent(
    onClose: () -> Unit,
    variant: StatusVariant,
    title: Int,
    body: Int?,
    action: Int,
    onAction: () -> Unit,
    onAsk: (() -> Unit)?,
) {
    Column(Modifier.fillMaxSize()) {
        LessonTopBar("", null, 0f, showIndexButton = false, onClose = onClose, onIndex = {})
        Column(
            Modifier.padding(screenMargin()).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
        ) {
            TeachStatusPanel(
                variant = variant,
                title = stringResource(title),
                body = body?.let { stringResource(it) },
                icon = if (variant == StatusVariant.Danger) TeachIcons.CloudOff else null,
            )
            TeachButton(stringResource(action), onAction, Modifier.fillMaxWidth())
            if (onAsk != null) {
                TeachButton(
                    stringResource(R.string.lesson_ask_tutor),
                    onAsk,
                    Modifier.fillMaxWidth(),
                    variant = ButtonVariant.Secondary,
                )
            }
        }
    }
}
