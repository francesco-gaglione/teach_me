package com.teachme.feature.folder

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ActionMenuItem
import com.teachme.core.designsystem.components.TeachActionMenu
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachPane
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.Flash
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.ResearchStatus
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicState
import com.teachme.core.ui.DeleteConfirmDialog
import com.teachme.core.ui.LoadErrorState
import com.teachme.core.ui.NameDialogTexts
import com.teachme.core.ui.NameInputDialog
import com.teachme.core.ui.TopicStateChip
import com.teachme.core.ui.topicStateDescription
import com.teachme.feature.chat.ChatLessonContext
import com.teachme.feature.chat.ChatPane
import com.teachme.feature.chat.ComposerFocusRequest
import com.teachme.feature.chat.LessonRequestCardSlot
import com.teachme.feature.chat.rememberChatViewModel
import com.teachme.feature.lesson.LessonOrigin
import com.teachme.feature.lesson.LessonRequestCard
import com.teachme.feature.lesson.LessonsEffect
import com.teachme.feature.lesson.LessonsTab
import com.teachme.feature.lesson.LessonsTabActions
import com.teachme.feature.lesson.ProgressSection
import com.teachme.feature.lesson.ProgressTab
import com.teachme.feature.lesson.ProgressTabActions
import com.teachme.feature.lesson.rememberLessonsViewModel
import com.teachme.feature.lesson.rememberProgressViewModel
import com.teachme.feature.mission.MissionChatContext
import com.teachme.feature.mission.MissionEffect
import com.teachme.feature.mission.MissionTab
import com.teachme.feature.mission.MissionTabActions
import com.teachme.feature.mission.MissionTabs
import com.teachme.feature.mission.TopicTab
import com.teachme.feature.mission.rememberMissionViewModel
import com.teachme.feature.resources.DocumentsEffect
import com.teachme.feature.resources.ResourcesEffect
import com.teachme.feature.resources.ResourcesTab
import com.teachme.feature.resources.ResourcesTabActions
import com.teachme.feature.resources.documentsActions
import com.teachme.feature.resources.rememberDocumentsViewModel
import com.teachme.feature.resources.rememberResourcesViewModel
import com.teachme.feature.resources.toPointer
import kotlinx.coroutines.launch
import java.time.Instant
import com.teachme.core.ui.R as UiR
import com.teachme.feature.lesson.R as LessonR
import com.teachme.feature.mission.R as MissionR
import com.teachme.feature.resources.R as ResR

/** What a lesson passes when it opens the chat of its Topic (`ask_agent`, "Chiedi al Tutor", "Ne parlo col Tutor"). */
data class TopicAskArgs(
    val prompt: String?,
    val lessonId: String,
    val lessonNumber: Int,
    val lessonTitle: String,
    val lessonMinutes: Int = 0,
)

/** Records shown in the context pane of the lessons tab (topic.md F8). */
private const val REGISTER_PANE_RECORDS = 3

/**
 * The topic screen (topic.md): always full window, on every width class.
 * [onLeave] is called when the topic is deleted or no longer exists; [onChanged] after a rename.
 */
@Composable
fun TopicPane(
    topicId: String,
    title: String,
    onBack: () -> Unit,
    onLeave: (Flash) -> Unit,
    onOpenSettings: () -> Unit,
    toast: TeachToastState,
    modifier: Modifier = Modifier,
    onChanged: () -> Unit = {},
    /** Opens the lesson with this id (the Lezioni tab, a record of "Cosa sai", a term of the glossary). */
    onOpenLesson: (String) -> Unit = {},
    /** Opens the review limited to this Topic ("Ripassa questo argomento"): topic id and title. */
    onOpenReview: (topicId: String, topicTitle: String) -> Unit = { _, _ -> },
    /** A tab asked for by the screen that opened this one (see [TopicTabNames]). */
    initialTab: String? = null,
    /** Set when this screen was opened from a lesson to ask the Tutor (`ask_agent`, "Chiedi al Tutor"). */
    ask: TopicAskArgs? = null,
    viewModel: TopicViewModel =
        hiltViewModel<TopicViewModel, TopicViewModel.Factory>(key = "topic-$topicId") { it.create(topicId, title) },
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val chatViewModel = rememberChatViewModel(topicId)
    val chatState by chatViewModel.state.collectAsStateWithLifecycle()
    val missionViewModel = rememberMissionViewModel(topicId)
    val missionState by missionViewModel.state.collectAsStateWithLifecycle()
    val resourcesViewModel = rememberResourcesViewModel(topicId)
    val resourcesState by resourcesViewModel.state.collectAsStateWithLifecycle()
    val documentsViewModel = rememberDocumentsViewModel(topicId)
    val documentsState by documentsViewModel.state.collectAsStateWithLifecycle()
    val readyPdfTitles =
        remember(documentsState.documents) {
            documentsState.documents.filter { it.status == DocumentStatus.Ready }.map { it.title }
        }
    val lessonsViewModel = rememberLessonsViewModel(topicId)
    val lessonsState by lessonsViewModel.state.collectAsStateWithLifecycle()
    val progressViewModel = rememberProgressViewModel(topicId)
    val progressState by progressViewModel.state.collectAsStateWithLifecycle()
    LaunchedEffect(initialTab) { tabNamed(initialTab)?.let(viewModel::selectTab) }
    LaunchedEffect(chatState.topic) { chatState.topic?.let(viewModel::onTopicFromChat) }
    val resources = LocalResources.current
    val currentOnLeave by rememberUpdatedState(onLeave)
    val currentOnChanged by rememberUpdatedState(onChanged)
    val loaded = state.topic != null
    LifecycleEventEffect(Lifecycle.Event.ON_START) { viewModel.onStart() }
    // The stream and the mission follow the screen, not the tab: switching tabs never closes the stream.
    LifecycleStartEffect(loaded) {
        if (loaded) {
            // The mission loads when the stream opens (the chat forwards that as a resync): one fetch, not two.
            chatViewModel.onStart()
            missionViewModel.loadIfStreamSilent()
            resourcesViewModel.loadIfStreamSilent()
            documentsViewModel.loadIfStreamSilent()
            lessonsViewModel.loadIfStreamSilent()
            // Back in the foreground the register and the glossary are read again (progress.md section 2).
            progressViewModel.refresh()
        }
        onStopOrDispose { chatViewModel.onStop() }
    }
    var focusComposerRequest by rememberSaveable { mutableStateOf<ComposerFocusRequest?>(null) }
    var prefsScrollPending by rememberSaveable { mutableStateOf(false) }
    var confirmedSignal by remember { mutableIntStateOf(0) }
    val prefsFocus = remember { FocusRequester() }
    val currentTab = state.tab ?: TopicTab.Chat
    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is TopicEffect.Renamed ->
                    launch {
                        toast.show(
                            resources.getString(R.string.folder_toast_topic_renamed, effect.title),
                        )
                    }
                is TopicEffect.Leave -> currentOnLeave(effect.flash)
                TopicEffect.Changed -> currentOnChanged()
            }
        }
    }
    // Mission and preference events arrive on the chat stream and are handed to the mission side.
    LaunchedEffect(
        chatViewModel,
        missionViewModel,
        resourcesViewModel,
        documentsViewModel,
        lessonsViewModel,
        progressViewModel,
    ) {
        chatViewModel.missionEvents.collect {
            missionViewModel.onEvent(it)
            resourcesViewModel.onEvent(it)
            documentsViewModel.onEvent(it)
            lessonsViewModel.onEvent(it)
            progressViewModel.onEvent(it)
        }
    }
    // The toast about the research depends on the tab being shown when it ends (resources.md section 7).
    val tabNow by rememberUpdatedState(state.tab ?: TopicTab.Chat)
    LaunchedEffect(resourcesViewModel) {
        resourcesViewModel.effects.collect { effect ->
            val onSources = tabNow == TopicTab.Resources
            when (effect) {
                is ResourcesEffect.Succeeded ->
                    if (!onSources) {
                        launch {
                            toast.show(
                                resources.getQuantityString(ResR.plurals.res_toast_found, effect.count, effect.count),
                                resources.getString(ResR.string.res_toast_see),
                            ) { viewModel.selectTab(TopicTab.Resources) }
                        }
                    } else if (effect.refresh) {
                        launch {
                            toast.show(
                                resources.getQuantityString(ResR.plurals.res_toast_updated, effect.count, effect.count),
                            )
                        }
                    }
                ResourcesEffect.Failed ->
                    if (!onSources) {
                        launch {
                            toast.show(
                                resources.getString(ResR.string.res_toast_failed),
                                resources.getString(ResR.string.res_toast_see),
                            ) { viewModel.selectTab(TopicTab.Resources) }
                        }
                    }
            }
        }
    }
    // PDFs: the toast says what happened wherever the learner is; the sources are read again after an import.
    LaunchedEffect(documentsViewModel) {
        documentsViewModel.effects.collect { effect ->
            when (effect) {
                is DocumentsEffect.Imported -> {
                    resourcesViewModel.refresh()
                    launch {
                        toast.show(
                            effect.pages?.let {
                                resources.getQuantityString(ResR.plurals.doc_toast_read, it, effect.title, it)
                            } ?: resources.getString(ResR.string.doc_toast_read_no_pages, effect.title),
                        )
                    }
                }
                DocumentsEffect.Removed -> launch { toast.show(resources.getString(ResR.string.doc_toast_removed)) }
                DocumentsEffect.RetryFailed ->
                    launch { toast.show(resources.getString(ResR.string.doc_toast_retry_failed)) }
                // The Topic is gone: reloading it takes the learner back to the Home with the usual message.
                DocumentsEffect.TopicGone -> viewModel.retry()
            }
        }
    }
    // The toast about a lesson depends on the tab being shown when it ends (topic.md F3.C): never on the Lessons tab.
    // Not either when the learner is looking at the chat and a card of that job is in it: the card is the notice
    // (chat.md design 10b.3).
    val chatMessagesNow by rememberUpdatedState(chatState.messages)
    LaunchedEffect(lessonsViewModel) {
        lessonsViewModel.effects.collect { effect ->
            if (tabNow == TopicTab.Lessons) return@collect
            val endedJob =
                when (effect) {
                    is LessonsEffect.Ready -> effect.jobId
                    is LessonsEffect.Failed -> effect.jobId
                }
            if (suppressLessonToast(tabNow, endedJob, chatMessagesNow)) return@collect
            when (effect) {
                is LessonsEffect.Ready ->
                    launch {
                        toast.show(
                            if (effect.number != null) {
                                resources.getString(LessonR.string.lessons_toast_ready, effect.number)
                            } else {
                                resources.getString(LessonR.string.lessons_toast_ready_plain)
                            },
                            resources.getString(LessonR.string.lessons_toast_see),
                        ) { viewModel.selectTab(TopicTab.Lessons) }
                    }
                is LessonsEffect.Failed ->
                    launch {
                        toast.show(
                            resources.getString(LessonR.string.lessons_toast_failed),
                            resources.getString(LessonR.string.lessons_toast_see),
                        ) { viewModel.selectTab(TopicTab.Lessons) }
                    }
            }
        }
    }
    LaunchedEffect(missionViewModel) {
        missionViewModel.effects.collect { effect ->
            when (effect) {
                MissionEffect.MissionChanged ->
                    launch { toast.show(resources.getString(MissionR.string.mission_toast_changed)) }
                MissionEffect.PreferenceSaved ->
                    launch {
                        toast.show(
                            resources.getString(MissionR.string.mission_toast_pref_saved),
                            resources.getString(MissionR.string.mission_toast_see),
                        ) {
                            viewModel.selectTab(TopicTab.Mission)
                            prefsScrollPending = true
                        }
                    }
                MissionEffect.PreferenceDeleted ->
                    launch { toast.show(resources.getString(MissionR.string.mission_toast_pref_deleted)) }
                MissionEffect.Confirmed -> confirmedSignal++
                MissionEffect.PreferenceRemovedFocus -> runCatching { prefsFocus.requestFocus() }
            }
        }
    }
    val topicState = state.topic?.state ?: TopicState.Unknown
    val research = resourcesState.toPointer(topicState)
    val researchFailed = research.status == ResearchStatus.Failed
    val openResources = { viewModel.selectTab(TopicTab.Resources) }
    val goToChat = {
        focusComposerRequest = ComposerFocusRequest.EditMission
        viewModel.selectTab(TopicTab.Chat)
    }
    val writeStartingPoint = {
        focusComposerRequest = ComposerFocusRequest.StartingPoint
        viewModel.selectTab(TopicTab.Chat)
    }
    val missionContext =
        MissionChatContext(
            topicTitle = state.title,
            mission = missionState.mission,
            confirm = missionState.confirm,
            turnInProgress = chatState.turnInProgress,
            research = research,
            onConfirm = missionViewModel::confirm,
            onEdit = goToChat,
            onOpenMission = { viewModel.selectTab(TopicTab.Mission) },
            onOpenResources = openResources,
        )
    val missionActions =
        MissionTabActions(
            onConfirm = missionViewModel::confirm,
            onGoToChat = goToChat,
            onWriteStartingPoint = writeStartingPoint,
            onRetryMission = missionViewModel::retryMission,
            onOpenSettings = onOpenSettings,
            onToggleVersions = missionViewModel::toggleVersions,
            onToggleVersion = missionViewModel::toggleVersion,
            onShowMoreVersions = missionViewModel::showMoreVersions,
            onRetryVersions = missionViewModel::retryVersions,
            onRetryPreferences = missionViewModel::retryPreferences,
            onDeletePreference = missionViewModel::requestDeletePreference,
            onConfirmDelete = missionViewModel::confirmDeletePreference,
            onDismissDelete = missionViewModel::dismissDeleteDialog,
            onOpenResources = openResources,
        )
    // The question from a lesson goes into the composer once; the chip stays until the message is sent or removed.
    var pendingPrefill by rememberSaveable { mutableStateOf(ask?.prompt?.takeIf { it.isNotBlank() }) }
    var lessonChipActive by rememberSaveable { mutableStateOf(ask != null) }
    val chatLessonContext =
        ask
            ?.takeIf {
                lessonChipActive
            }?.let { ChatLessonContext(it.lessonId, it.lessonNumber, it.lessonTitle, it.lessonMinutes) }
    // The "Lezione" block of the context pane lasts for the whole visit, even after the chip is gone (ask_agent.md).
    val visitLesson = ask?.let { ChatLessonContext(it.lessonId, it.lessonNumber, it.lessonTitle, it.lessonMinutes) }
    val originOf: (String?) -> LessonOrigin? = { id ->
        lessonsState.lesson(id)?.let { LessonOrigin(it.id, it.number, it.title) }
    }
    val lessonsActions =
        LessonsTabActions(
            onGenerate = lessonsViewModel::generate,
            onRetryGeneration = lessonsViewModel::retry,
            onRetryLoad = lessonsViewModel::retryLoad,
            onLoadMore = lessonsViewModel::loadMore,
            onOpenLesson = { onOpenLesson(it.id) },
            onGoToChat = goToChat,
            onOpenMission = { viewModel.selectTab(TopicTab.Mission) },
            onOpenResources = openResources,
            onOpenRegister = {
                progressViewModel.selectSection(ProgressSection.Register)
                viewModel.selectTab(TopicTab.Progress)
            },
            onOpenSettings = onOpenSettings,
            onLeave = lessonsViewModel::markSeen,
            onOpenReview = { onOpenReview(topicId, state.title) },
        )
    // The card of a lesson asked for in the chat follows its job in the lessons state (no second channel). The slot is
    // remembered and reads the state through a snapshot holder: a `job_updated` recomposes only the card concerned.
    val lessonsStateHolder = rememberUpdatedState(lessonsState)
    val openLessonNow by rememberUpdatedState(onOpenLesson)
    val lessonRequestCard: LessonRequestCardSlot =
        remember<LessonRequestCardSlot>(lessonsViewModel) {
            { jobId, announces ->
                LessonRequestCard(
                    jobId = jobId,
                    stateProvider = { lessonsStateHolder.value },
                    announces = announces,
                    onOpenLesson = { lessonId ->
                        // As if the Lessons tab had been visited: the "Nuova" mark goes out (topic.md F3.C).
                        lessonsViewModel.markSeen()
                        openLessonNow(lessonId)
                    },
                    onRetry = lessonsViewModel::retry,
                )
            }
        }
    val progressActions =
        ProgressTabActions(
            onSelectSection = progressViewModel::selectSection,
            onRetryRecords = progressViewModel::retryRecords,
            onRetryGlossary = progressViewModel::retryGlossary,
            onOpenLesson = onOpenLesson,
            onGoToLessons = { viewModel.selectTab(TopicTab.Lessons) },
            onOpenSettings = onOpenSettings,
        )
    val resourcesActions =
        ResourcesTabActions(
            onRetryLoad = resourcesViewModel::retryLoad,
            onRetry = resourcesViewModel::retry,
            onStartResearch = resourcesViewModel::startResearch,
            onGoToChat = goToChat,
            onOpenSettings = onOpenSettings,
            onLeave = resourcesViewModel::markSeen,
        )
    TopicPaneContent(
        state = state,
        turnInProgress = chatState.turnInProgress,
        hasMission = missionState.mission?.let { it.current != null || it.draft != null } == true,
        researchFailed = researchFailed,
        hasLessons = lessonsState.lessons.isNotEmpty(),
        lessonPreparing = lessonsState.job?.isActive == true,
        backDescription = ask?.let { stringResource(R.string.topic_back_to_lesson, it.lessonTitle) },
        toast = toast,
        onBack = onBack,
        body = { tab, header, headerScrolls ->
            when (tab) {
                TopicTab.Lessons ->
                    Column(Modifier.fillMaxSize()) {
                        if (!headerScrolls) header()
                        LessonsTab(
                            topicState = topicState,
                            researchFailed = researchFailed,
                            state = lessonsState,
                            missionWhy = missionState.current?.why,
                            recentRecords = progressState.activeRecords.take(REGISTER_PANE_RECORDS),
                            actions = lessonsActions,
                            pdfTitles = readyPdfTitles,
                            review = state.review,
                            modifier = Modifier.weight(1f),
                            header = header.takeIf { headerScrolls },
                        )
                    }
                TopicTab.Progress ->
                    Column(Modifier.fillMaxSize()) {
                        if (!headerScrolls) header()
                        ProgressTab(
                            state = progressState,
                            actions = progressActions,
                            originOf = originOf,
                            hasUnfinishedLesson = lessonsState.lessons.any { it.status != LessonStatus.Completed },
                            allLessonsCompleted =
                                lessonsState.lessons.isNotEmpty() &&
                                    lessonsState.lessons.all { it.status == LessonStatus.Completed },
                            modifier = Modifier.weight(1f),
                            header = header.takeIf { headerScrolls },
                        )
                    }
                TopicTab.Chat ->
                    ChatPane(
                        topicTitle = state.title,
                        viewModel = chatViewModel,
                        toast = toast,
                        onOpenSettings = onOpenSettings,
                        header = header,
                        headerScrolls = headerScrolls,
                        mission = missionContext,
                        onTopicMaybeGone = viewModel::retry,
                        focusComposerRequest = focusComposerRequest,
                        onFocusComposerHandled = { focusComposerRequest = null },
                        lessonContext = chatLessonContext,
                        onLessonContextConsumed = { lessonChipActive = false },
                        visitLesson = visitLesson,
                        onBackToLesson = onBack.takeIf { visitLesson != null },
                        prefill = pendingPrefill,
                        onPrefillConsumed = { pendingPrefill = null },
                        lessonRequestCard = lessonRequestCard,
                    )
                TopicTab.Resources ->
                    Column(Modifier.fillMaxSize()) {
                        if (!headerScrolls) header()
                        ResourcesTab(
                            topicState = topicState,
                            missionWhy = missionState.current?.why,
                            currentMissionVersion = missionState.current?.version,
                            hasProposal = topicState == TopicState.MissionProposed,
                            state = resourcesState,
                            actions = resourcesActions,
                            documents = documentsState.copy(topicLanguage = state.topic?.language ?: TopicLanguage.It),
                            documentActions = documentsActions(documentsViewModel, onOpenSettings),
                            toast = toast,
                            modifier = Modifier.weight(1f),
                            header = header.takeIf { headerScrolls },
                        )
                    }
                TopicTab.Mission ->
                    Column(Modifier.fillMaxSize()) {
                        if (!headerScrolls) header()
                        MissionTab(
                            topicTitle = state.title,
                            research = research,
                            state = missionState,
                            turnInProgress = chatState.turnInProgress,
                            actions = missionActions,
                            pdfTitles = readyPdfTitles,
                            modifier = Modifier.weight(1f),
                            header = header.takeIf { headerScrolls },
                            scrollToPreferences = prefsScrollPending,
                            onScrolledToPreferences = { prefsScrollPending = false },
                            confirmedSignal = confirmedSignal,
                            prefsTitleFocus = prefsFocus,
                        )
                    }
            }
        },
        onSelectTab = viewModel::selectTab,
        onOpenSettings = onOpenSettings,
        onRetry = viewModel::retry,
        onRename = viewModel::openRename,
        onDelete = viewModel::openDelete,
        onDismissDialog = viewModel::dismissDialog,
        onDialogTextEdited = viewModel::onDialogTextEdited,
        onSubmitRename = viewModel::rename,
        onConfirmDelete = viewModel::confirmDelete,
        modifier = modifier,
    )
}

@Composable
internal fun TopicPaneContent(
    state: TopicUiState,
    turnInProgress: Boolean,
    toast: TeachToastState,
    onBack: () -> Unit,
    body: @Composable (tab: TopicTab, header: @Composable () -> Unit, headerScrolls: Boolean) -> Unit,
    modifier: Modifier = Modifier,
    hasMission: Boolean = false,
    researchFailed: Boolean = false,
    hasLessons: Boolean = false,
    lessonPreparing: Boolean = false,
    /** Replaces "Indietro" when the screen was opened from a lesson: "Torna alla lezione «…»". */
    backDescription: String? = null,
    onSelectTab: (TopicTab) -> Unit = {},
    onOpenSettings: () -> Unit = {},
    onRetry: () -> Unit = {},
    onRename: () -> Unit = {},
    onDelete: () -> Unit = {},
    onDismissDialog: () -> Unit = {},
    onDialogTextEdited: () -> Unit = {},
    onSubmitRename: (String) -> Unit = {},
    onConfirmDelete: () -> Unit = {},
) {
    var menu by rememberSaveable { mutableStateOf(false) }
    val topic = state.topic
    val header: @Composable () -> Unit = {
        Column {
            TeachTopBar(
                title = state.title,
                navigation = {
                    TeachIconButton(
                        TeachIcons.ChevronLeft,
                        backDescription ?: stringResource(R.string.topic_back),
                        onBack,
                        tint = TeachTheme.colors.ink,
                    )
                },
                actions = {
                    Box {
                        TeachIconButton(
                            TeachIcons.MoreVertical,
                            stringResource(R.string.topic_more_actions),
                            { menu = true },
                            enabled = topic != null,
                        )
                        TeachActionMenu(
                            menu,
                            { menu = false },
                            stringResource(UiR.string.ui_actions_for, state.title),
                            listOf(
                                ActionMenuItem(
                                    stringResource(R.string.topic_action_rename),
                                    TeachIcons.Pencil,
                                    onRename,
                                ),
                                ActionMenuItem(
                                    stringResource(R.string.topic_action_delete),
                                    TeachIcons.Trash,
                                    onDelete,
                                    destructive = true,
                                ),
                            ),
                        )
                    }
                },
            )
            if (topic != null) {
                val description =
                    stringResource(
                        R.string.topic_chip_description,
                        topicStateDescription(topic.state, researchFailed = researchFailed).substringAfter(": "),
                    )
                Row(
                    modifier = Modifier.padding(start = screenMargin()),
                    horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TopicStateChip(
                        topic.state,
                        researchFailed = researchFailed,
                        modifier = Modifier.semantics { contentDescription = description },
                        decorative = false,
                    )
                    TeachText(
                        stringResource(R.string.topic_language_line, topicLanguageName(topic.language)),
                        style = TeachTheme.typography.ui.bodyS,
                        color = TeachTheme.colors.inkSoft,
                    )
                }
                MissionTabs(
                    selected = state.tab ?: TopicTab.Chat,
                    onSelect = onSelectTab,
                    modifier = Modifier.padding(top = TeachTheme.spacing.xs),
                    // The labels carry their own 16 dp: the text lines up with the chip.
                    startPadding = (screenMargin() - TeachTheme.spacing.md).coerceAtLeast(0.dp),
                )
            }
        }
    }
    val currentTab = state.tab ?: TopicTab.Chat
    BoxWithConstraints(
        modifier.onPreviewKeyEvent { event ->
            if (topic == null) return@onPreviewKeyEvent false
            val target = tabForKey(event, currentTab) ?: return@onPreviewKeyEvent false
            onSelectTab(target)
            true
        },
    ) {
        // Short usable height (virtual keyboard in landscape): top bar and chip scroll away with the list.
        val headerScrolls = maxHeight < SHORT_WINDOW_HEIGHT
        TeachPane(
            modifier = Modifier.fillMaxSize(),
            topBar = { if (topic == null) header() },
            toast = { TeachToastHost(toast) },
        ) {
            when {
                state.loadError != null && topic == null ->
                    LoadErrorState(state.loadError, onRetry = onRetry, onOpenSettings = onOpenSettings)
                topic == null ->
                    TeachText(
                        stringResource(R.string.topic_loading),
                        modifier = Modifier.padding(screenMargin()),
                        style = TeachTheme.typography.ui.bodyL,
                        color = TeachTheme.colors.inkSoft,
                    )
                else -> {
                    // Each tab keeps its own scroll, draft and expanded sections while the other is shown.
                    val holder = rememberSaveableStateHolder()
                    val tab = state.tab ?: TopicTab.Chat
                    holder.SaveableStateProvider(tab.name) { body(tab, header, headerScrolls) }
                }
            }
            when (val dialog = state.dialog) {
                null -> Unit
                is TopicDialog.Rename ->
                    NameInputDialog(
                        NameDialogTexts(
                            title = stringResource(R.string.folder_rename_topic_title),
                            label = stringResource(R.string.folder_topic_label),
                            placeholder = stringResource(R.string.folder_topic_placeholder),
                            confirm = stringResource(R.string.folder_rename_confirm),
                            loading = stringResource(R.string.folder_rename_loading),
                            empty = stringResource(R.string.folder_topic_empty),
                            duplicate = stringResource(R.string.folder_topic_duplicate),
                            notDone = stringResource(R.string.folder_rename_topic_not_done),
                        ),
                        TOPIC_TITLE_MAX,
                        dialog.ui,
                        onSubmitRename,
                        onDismissDialog,
                        onDialogTextEdited,
                        onOpenSettings,
                        initialText = state.title,
                    )
                is TopicDialog.Delete ->
                    DeleteConfirmDialog(
                        title = stringResource(R.string.folder_delete_topic_title, state.title),
                        body =
                            listOfNotNull(
                                stringResource(
                                    when {
                                        hasMission && turnInProgress -> R.string.folder_delete_topic_body_mission_turn
                                        hasMission -> R.string.folder_delete_topic_body_mission
                                        turnInProgress -> R.string.folder_delete_topic_body_turn
                                        else -> R.string.folder_delete_topic_body
                                    },
                                ),
                                stringResource(R.string.folder_delete_topic_extra_lessons).takeIf { hasLessons },
                                stringResource(R.string.folder_delete_topic_extra_preparing).takeIf { lessonPreparing },
                            ).joinToString(" "),
                        confirmLabel = stringResource(R.string.folder_delete_topic_confirm),
                        loadingLabel = stringResource(R.string.folder_delete_loading),
                        notDone = stringResource(R.string.folder_delete_topic_not_done),
                        ui = dialog.ui,
                        onConfirm = onConfirmDelete,
                        onDismiss = onDismissDialog,
                        onOpenSettings = onOpenSettings,
                    )
            }
        }
    }
}

/**
 * Hardware keyboard shortcuts of the tab row (topic.md F1): Ctrl+Tab / Ctrl+Shift+Tab cycle, Alt+1..5 jump.
 * Returns the tab to select, or null when the event is not a shortcut.
 */
internal fun tabForKey(
    event: KeyEvent,
    current: TopicTab,
): TopicTab? {
    if (event.type != KeyEventType.KeyDown) return null
    val tabs = TopicTab.entries
    return when {
        event.isCtrlPressed && event.key == Key.Tab -> {
            val step = if (event.isShiftPressed) tabs.size - 1 else 1
            tabs[(tabs.indexOf(current) + step) % tabs.size]
        }
        event.isAltPressed && !event.isCtrlPressed -> {
            val index =
                when (event.key) {
                    Key.One -> 0
                    Key.Two -> 1
                    Key.Three -> 2
                    Key.Four -> 3
                    Key.Five -> 4
                    else -> return null
                }
            tabs.getOrNull(index)
        }
        else -> null
    }
}

/** Below this usable height the top bar scrolls with the chat (topic.md). */
private val SHORT_WINDOW_HEIGHT = 480.dp

private val previewTopic = Topic("1", "f", "Rust", TopicState.New, Instant.EPOCH, Instant.EPOCH)

@Composable
private fun PreviewHost(state: TopicUiState) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        TopicPaneContent(
            state,
            turnInProgress = false,
            toast = remember { TeachToastState() },
            onBack = {},
            body = { _, header, _ ->
                Column {
                    header()
                    TeachText("(chat)", Modifier.padding(screenMargin()), color = TeachTheme.colors.inkSoft)
                }
            },
        )
    }
}

@Preview(name = "Topic", showBackground = true, heightDp = 400)
@Composable
private fun TopicPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(TopicUiState("Rust", previewTopic, loading = false))
    }

@Preview(name = "Topic dark ready", showBackground = true, heightDp = 400)
@Composable
private fun TopicDarkPreview() =
    TeachTheme(darkTheme = true) {
        PreviewHost(TopicUiState("Rust", previewTopic.copy(state = TopicState.Ready), loading = false))
    }

@Preview(name = "Topic loading", showBackground = true, heightDp = 300)
@Composable
private fun TopicLoadingPreview() = TeachTheme(darkTheme = false) { PreviewHost(TopicUiState("Rust")) }

@Preview(name = "Topic error", showBackground = true, heightDp = 400)
@Composable
private fun TopicErrorPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(TopicUiState("Rust", loading = false, loadError = LoadError.Network))
    }

@Preview(name = "Topic font 2.0", showBackground = true, heightDp = 700, fontScale = 2f)
@Composable
private fun TopicLargePreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(TopicUiState("Rust", previewTopic, loading = false))
    }
