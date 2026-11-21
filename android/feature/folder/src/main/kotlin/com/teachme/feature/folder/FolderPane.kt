package com.teachme.feature.folder

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ActionMenuItem
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.TeachActionMenu
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachCard
import com.teachme.core.designsystem.components.TeachErrorBanner
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachListRow
import com.teachme.core.designsystem.components.TeachListSkeleton
import com.teachme.core.designsystem.components.TeachPane
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastHost
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.designsystem.screenMargin
import com.teachme.core.model.Flash
import com.teachme.core.model.LoadError
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicState
import com.teachme.core.ui.DeleteConfirmDialog
import com.teachme.core.ui.LoadErrorState
import com.teachme.core.ui.NameDialogTexts
import com.teachme.core.ui.NameInputDialog
import com.teachme.core.ui.TopicStateChip
import com.teachme.core.ui.rememberRowRequesters
import com.teachme.core.ui.topicCountText
import com.teachme.core.ui.topicStateDescription
import kotlinx.coroutines.launch
import java.time.Instant
import com.teachme.core.ui.R as UiR

private val BOTTOM_BUTTON_SPACE = 88.dp
private val TOAST_LIFT = 72.dp

/**
 * Topics of a folder (folder.md). Compact/medium: a full screen with back arrow. Expanded ([wide]): the
 * right pane of the list-detail layout, with its own header (name, count, menu, primary "Nuovo argomento").
 * [onLeave] is called when the folder no longer exists (deleted here or elsewhere); [onChanged] when
 * something other panes display (name, counts) changed; [refreshKey] bumps ask for a silent reload.
 */
@Composable
fun FolderPane(
    folderId: String,
    folderName: String,
    onOpenTopic: (Topic) -> Unit,
    onLeave: (Flash) -> Unit,
    onOpenSettings: () -> Unit,
    toast: TeachToastState,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
    onChanged: () -> Unit = {},
    wide: Boolean = false,
    refreshKey: Int = 0,
    viewModel: FolderViewModel =
        hiltViewModel<FolderViewModel, FolderViewModel.Factory>(
            key = "folder-$folderId",
        ) { it.create(folderId, folderName) },
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var pendingScroll by remember { mutableStateOf<String?>(null) }
    val resources = LocalResources.current
    val currentOpenTopic by rememberUpdatedState(onOpenTopic)
    val currentOnLeave by rememberUpdatedState(onLeave)
    val currentOnChanged by rememberUpdatedState(onChanged)

    LifecycleEventEffect(Lifecycle.Event.ON_START) { viewModel.onStart() }
    LaunchedEffect(refreshKey) { if (refreshKey != 0) viewModel.retry() }
    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is FolderEffect.Toast ->
                    launch {
                        when (val m = effect.message) {
                            is FolderMessage.TopicCreated ->
                                toast.show(
                                    resources.getString(R.string.folder_toast_topic_created, m.topic.title),
                                    resources.getString(R.string.folder_toast_open),
                                ) { currentOpenTopic(m.topic) }
                            is FolderMessage.TopicRenamed ->
                                toast.show(
                                    resources.getString(R.string.folder_toast_topic_renamed, m.title),
                                )
                            is FolderMessage.TopicDeleted ->
                                toast.show(
                                    resources.getString(R.string.folder_toast_topic_deleted, m.title),
                                )
                            FolderMessage.TopicGone -> toast.show(resources.getString(R.string.folder_toast_topic_gone))
                            is FolderMessage.FolderRenamed ->
                                toast.show(
                                    resources.getString(R.string.folder_toast_folder_renamed, m.name),
                                )
                        }
                    }
                is FolderEffect.ScrollTo -> pendingScroll = effect.topicId
                is FolderEffect.Leave -> currentOnLeave(effect.flash)
                FolderEffect.Changed -> currentOnChanged()
                is FolderEffect.TopicRemoved -> Unit
            }
        }
    }

    FolderPaneContent(
        state = state,
        toast = toast,
        wide = wide,
        onBack = onBack,
        pendingScroll = pendingScroll,
        onScrolled = { pendingScroll = null },
        actions =
            FolderActions(
                onOpenTopic = onOpenTopic,
                onOpenSettings = onOpenSettings,
                onRetry = viewModel::retry,
                onDismissRefreshError = viewModel::dismissRefreshError,
                onCreateTopic = { viewModel.openCreateTopic() },
                onSelectCreateLanguage = viewModel::selectCreateLanguage,
                onRenameTopic = viewModel::openRenameTopic,
                onDeleteTopic = viewModel::openDeleteTopic,
                onRenameFolder = viewModel::openRenameFolder,
                onDeleteFolder = viewModel::openDeleteFolder,
                onDismissDialog = viewModel::dismissDialog,
                onDialogTextEdited = viewModel::onDialogTextEdited,
                onSubmitCreateTopic = viewModel::createTopic,
                onSubmitRenameTopic = viewModel::renameTopic,
                onConfirmDeleteTopic = viewModel::confirmDeleteTopic,
                onSubmitRenameFolder = viewModel::renameFolder,
                onConfirmDeleteFolder = viewModel::confirmDeleteFolder,
            ),
        modifier = modifier,
    )
}

data class FolderActions(
    val onOpenTopic: (Topic) -> Unit = {},
    val onOpenSettings: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onDismissRefreshError: () -> Unit = {},
    val onCreateTopic: () -> Unit = {},
    val onSelectCreateLanguage: (TopicLanguage) -> Unit = {},
    val onRenameTopic: (Topic) -> Unit = {},
    val onDeleteTopic: (Topic) -> Unit = {},
    val onRenameFolder: () -> Unit = {},
    val onDeleteFolder: () -> Unit = {},
    val onDismissDialog: () -> Unit = {},
    val onDialogTextEdited: () -> Unit = {},
    val onSubmitCreateTopic: (String) -> Unit = {},
    val onSubmitRenameTopic: (String) -> Unit = {},
    val onConfirmDeleteTopic: () -> Unit = {},
    val onSubmitRenameFolder: (String) -> Unit = {},
    val onConfirmDeleteFolder: () -> Unit = {},
)

@Composable
internal fun FolderPaneContent(
    state: FolderUiState,
    toast: TeachToastState,
    actions: FolderActions,
    modifier: Modifier = Modifier,
    wide: Boolean = false,
    onBack: (() -> Unit)? = null,
    pendingScroll: String? = null,
    onScrolled: () -> Unit = {},
) {
    var folderMenu by rememberSaveable { mutableStateOf(false) }
    val folderMenuItems =
        listOf(
            ActionMenuItem(
                stringResource(R.string.folder_action_rename_folder),
                TeachIcons.Pencil,
                actions.onRenameFolder,
            ),
            ActionMenuItem(
                stringResource(R.string.folder_action_delete_folder),
                TeachIcons.Trash,
                actions.onDeleteFolder,
                destructive = true,
            ),
        )
    val paneTitle = stringResource(R.string.folder_pane_title, state.folderName)
    TeachPane(
        modifier = modifier.then(if (wide) Modifier.semantics { this.paneTitle = paneTitle } else Modifier),
        topBar = {
            if (!wide) {
                TeachTopBar(
                    title = state.folderName,
                    navigation =
                        onBack?.let {
                            {
                                TeachIconButton(
                                    TeachIcons.ChevronLeft,
                                    stringResource(R.string.folder_back),
                                    it,
                                    tint = TeachTheme.colors.ink,
                                )
                            }
                        },
                    actions = {
                        Box {
                            TeachIconButton(TeachIcons.MoreVertical, stringResource(R.string.folder_more_actions), {
                                folderMenu =
                                    true
                            })
                            TeachActionMenu(folderMenu, {
                                folderMenu = false
                            }, stringResource(UiR.string.ui_actions_for, state.folderName), folderMenuItems)
                        }
                    },
                )
            }
        },
        toast = {
            Box(
                Modifier.padding(
                    bottom =
                        if (wide ||
                            state.topics.isNullOrEmpty()
                        ) {
                            0.dp
                        } else {
                            TOAST_LIFT
                        },
                ),
            ) { TeachToastHost(toast) }
        },
    ) {
        val topics = state.topics
        Column(Modifier.fillMaxSize()) {
            if (wide) WideHeader(state, actions, folderMenu, { folderMenu = it }, folderMenuItems)
            Box(Modifier.weight(1f)) {
                when {
                    topics == null && state.loadError != null ->
                        LoadErrorState(
                            state.loadError,
                            onRetry = actions.onRetry,
                            onOpenSettings = actions.onOpenSettings,
                        )
                    topics == null -> LoadingTopics(state.slowLoading)
                    topics.isEmpty() -> EmptyTopics(state, actions)
                    else -> TopicList(topics, state, actions, wide, pendingScroll, onScrolled)
                }
            }
        }
        FolderDialogs(state, actions)
    }
}

@Composable
private fun WideHeader(
    state: FolderUiState,
    actions: FolderActions,
    menuOpen: Boolean,
    onMenu: (Boolean) -> Unit,
    items: List<ActionMenuItem>,
) {
    val margin = TeachTheme.layout.screenMarginExpanded
    val showCreate = state.topics?.isNotEmpty() == true
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .heightIn(
                    min = TeachTheme.layout.paneHeaderHeight,
                ).padding(horizontal = margin, vertical = TeachTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
    ) {
        Column(Modifier.weight(1f)) {
            TeachText(
                state.folderName,
                modifier = Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.title1,
            )
            state.topicCount?.let {
                TeachText(topicCountText(it), style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
            }
        }
        Box {
            TeachIconButton(TeachIcons.MoreVertical, stringResource(R.string.folder_more_actions), { onMenu(true) })
            TeachActionMenu(
                menuOpen,
                { onMenu(false) },
                stringResource(UiR.string.ui_actions_for, state.folderName),
                items,
            )
        }
        if (showCreate) {
            TeachButton(stringResource(R.string.folder_new_topic), actions.onCreateTopic, leadingIcon = TeachIcons.Plus)
        }
    }
}

@Composable
private fun LoadingTopics(slow: Boolean) {
    Column(
        Modifier.fillMaxSize().padding(horizontal = screenMargin()).widthIn(max = TeachTheme.layout.contentMaxWidth),
    ) {
        SectionHeading()
        TeachListSkeleton(
            description = stringResource(R.string.folder_loading_topics),
            stateText = stringResource(UiR.string.ui_loading_state),
            withIcon = false,
            slowText = if (slow) stringResource(UiR.string.ui_still_loading) else null,
        )
    }
}

@Composable
private fun SectionHeading() {
    TeachText(
        stringResource(R.string.folder_section_topics),
        modifier =
            Modifier
                .padding(
                    top = TeachTheme.spacing.lg,
                    bottom = TeachTheme.spacing.xs,
                ).semantics { heading() },
        style = TeachTheme.typography.ui.label,
        color = TeachTheme.colors.inkSoft,
    )
}

@Composable
private fun EmptyTopics(
    state: FolderUiState,
    actions: FolderActions,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(screenMargin()),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (state.refreshFailed) {
            TeachErrorBanner(
                stringResource(R.string.folder_refresh_failed),
                stringResource(UiR.string.ui_load_retry),
                actions.onRetry,
                stringResource(UiR.string.ui_load_dismiss),
                actions.onDismissRefreshError,
            )
        }
        Column(
            Modifier.widthIn(max = TeachTheme.layout.readingMaxWidth),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            TeachText(
                stringResource(R.string.folder_empty_title),
                modifier =
                    Modifier.semantics {
                        heading()
                    },
                style = TeachTheme.typography.ui.title1,
            )
            TeachText(
                stringResource(R.string.folder_empty_body),
                style = TeachTheme.typography.ui.bodyL,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(stringResource(R.string.folder_empty_cta), actions.onCreateTopic, size = ButtonSize.Large)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TopicList(
    topics: List<Topic>,
    state: FolderUiState,
    actions: FolderActions,
    wide: Boolean,
    pendingScroll: String?,
    onScrolled: () -> Unit,
) {
    val requesters = rememberRowRequesters()
    var menuFor by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(pendingScroll, topics) {
        val id = pendingScroll ?: return@LaunchedEffect
        if (topics.any { it.id == id }) {
            requesters.of(id).bringIntoView()
            onScrolled()
        }
    }
    val bottomSpace = if (wide) 24.dp else BOTTOM_BUTTON_SPACE * LocalDensity.current.fontScale
    Box(Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier
                    .padding(
                        horizontal = screenMargin(),
                    ).padding(bottom = bottomSpace)
                    .widthIn(max = TeachTheme.layout.contentMaxWidth)
                    .fillMaxWidth(),
            ) {
                if (state.refreshFailed) {
                    TeachErrorBanner(
                        stringResource(R.string.folder_refresh_failed),
                        stringResource(UiR.string.ui_load_retry),
                        actions.onRetry,
                        stringResource(UiR.string.ui_load_dismiss),
                        actions.onDismissRefreshError,
                        modifier = Modifier.padding(top = TeachTheme.spacing.md),
                    )
                }
                if (!wide) SectionHeading() else Box(Modifier.padding(top = TeachTheme.spacing.xs))
                TeachCard(Modifier.fillMaxWidth()) {
                    Column {
                        topics.forEachIndexed { index, topic ->
                            if (index > 0) TeachRowDivider()
                            TopicRow(
                                topic,
                                menuOpen = menuFor == topic.id,
                                onMenuOpen = { menuFor = topic.id },
                                onMenuClose = { menuFor = null },
                                actions = actions,
                                modifier =
                                    Modifier.bringIntoViewRequester(
                                        requesters.of(topic.id),
                                    ),
                            )
                        }
                    }
                }
            }
        }
        if (!wide) {
            TeachButton(
                stringResource(R.string.folder_new_topic),
                actions.onCreateTopic,
                modifier =
                    Modifier
                        .align(
                            Alignment.BottomCenter,
                        ).padding(TeachTheme.layout.screenMarginCompact)
                        .fillMaxWidth()
                        .widthIn(max = 480.dp),
                size = ButtonSize.Large,
                leadingIcon = TeachIcons.Plus,
            )
        }
    }
}

@Composable
private fun TopicRow(
    topic: Topic,
    menuOpen: Boolean,
    onMenuOpen: () -> Unit,
    onMenuClose: () -> Unit,
    actions: FolderActions,
    modifier: Modifier = Modifier,
) {
    val renameLabel = stringResource(UiR.string.ui_action_rename)
    val deleteLabel = stringResource(UiR.string.ui_action_delete)
    Box(modifier) {
        TeachListRow(
            title = topic.title,
            onClick = { actions.onOpenTopic(topic) },
            contentDescription =
                stringResource(
                    R.string.folder_row_description,
                    topic.title,
                    topicStateDescription(topic.state),
                ),
            supporting = { TopicStateChip(topic.state) },
            onLongClick = onMenuOpen,
            customActions =
                listOf(
                    CustomAccessibilityAction(renameLabel) {
                        actions.onRenameTopic(topic)
                        true
                    },
                    CustomAccessibilityAction(deleteLabel) {
                        actions.onDeleteTopic(topic)
                        true
                    },
                ),
            trailing = {
                TeachIconButton(
                    TeachIcons.MoreVertical,
                    stringResource(UiR.string.ui_more_actions_for, topic.title),
                    onMenuOpen,
                )
            },
        )
        TeachActionMenu(
            expanded = menuOpen,
            onDismiss = onMenuClose,
            title = stringResource(UiR.string.ui_actions_for, topic.title),
            items =
                listOf(
                    ActionMenuItem(renameLabel, TeachIcons.Pencil, { actions.onRenameTopic(topic) }),
                    ActionMenuItem(deleteLabel, TeachIcons.Trash, { actions.onDeleteTopic(topic) }, destructive = true),
                ),
        )
    }
}

@Composable
private fun FolderDialogs(
    state: FolderUiState,
    actions: FolderActions,
) {
    when (val dialog = state.dialog) {
        null -> Unit
        is FolderDialog.CreateTopic ->
            NameInputDialog(
                topicTexts(rename = false),
                TOPIC_TITLE_MAX,
                dialog.ui,
                actions.onSubmitCreateTopic,
                actions.onDismissDialog,
                actions.onDialogTextEdited,
                actions.onOpenSettings,
                extraContent = {
                    TopicLanguagePicker(
                        dialog.language,
                        actions.onSelectCreateLanguage,
                        enabled = !dialog.ui.submitting,
                    )
                },
            )
        is FolderDialog.RenameTopic ->
            NameInputDialog(
                topicTexts(rename = true),
                TOPIC_TITLE_MAX,
                dialog.ui,
                actions.onSubmitRenameTopic,
                actions.onDismissDialog,
                actions.onDialogTextEdited,
                actions.onOpenSettings,
                initialText = dialog.topic.title,
            )
        is FolderDialog.DeleteTopic ->
            DeleteConfirmDialog(
                title = stringResource(R.string.folder_delete_topic_title, dialog.topic.title),
                body = stringResource(R.string.folder_delete_topic_body),
                confirmLabel = stringResource(R.string.folder_delete_topic_confirm),
                loadingLabel = stringResource(R.string.folder_delete_loading),
                notDone = stringResource(R.string.folder_delete_topic_not_done),
                ui = dialog.ui,
                onConfirm = actions.onConfirmDeleteTopic,
                onDismiss = actions.onDismissDialog,
                onOpenSettings = actions.onOpenSettings,
            )
        is FolderDialog.RenameFolder ->
            NameInputDialog(
                folderTexts(),
                FOLDER_NAME_MAX,
                dialog.ui,
                actions.onSubmitRenameFolder,
                actions.onDismissDialog,
                actions.onDialogTextEdited,
                actions.onOpenSettings,
                initialText = state.folderName,
            )
        is FolderDialog.DeleteFolder -> {
            val count = state.topicCount ?: 0
            DeleteConfirmDialog(
                title = stringResource(R.string.folder_delete_folder_title, state.folderName),
                body =
                    when (count) {
                        0 -> stringResource(R.string.folder_delete_folder_body_empty)
                        1 -> stringResource(R.string.folder_delete_folder_body_one)
                        else -> stringResource(R.string.folder_delete_folder_body_many, count)
                    },
                confirmLabel =
                    stringResource(
                        when (count) {
                            0 -> R.string.folder_delete_folder_confirm_empty
                            1 -> R.string.folder_delete_folder_confirm_one
                            else -> R.string.folder_delete_folder_confirm_many
                        },
                    ),
                loadingLabel = stringResource(R.string.folder_delete_loading),
                notDone = stringResource(R.string.folder_delete_folder_not_done),
                ui = dialog.ui,
                onConfirm = actions.onConfirmDeleteFolder,
                onDismiss = actions.onDismissDialog,
                onOpenSettings = actions.onOpenSettings,
            )
        }
    }
}

@Composable
private fun topicTexts(rename: Boolean) =
    NameDialogTexts(
        title = stringResource(if (rename) R.string.folder_rename_topic_title else R.string.folder_create_topic_title),
        label = stringResource(R.string.folder_topic_label),
        placeholder = stringResource(R.string.folder_topic_placeholder),
        confirm = stringResource(if (rename) R.string.folder_rename_confirm else R.string.folder_create_confirm),
        loading = stringResource(if (rename) R.string.folder_rename_loading else R.string.folder_create_loading),
        empty = stringResource(R.string.folder_topic_empty),
        duplicate = stringResource(R.string.folder_topic_duplicate),
        notDone =
            stringResource(
                if (rename) R.string.folder_rename_topic_not_done else R.string.folder_create_topic_not_done,
            ),
    )

@Composable
private fun folderTexts() =
    NameDialogTexts(
        title = stringResource(R.string.folder_rename_folder_title),
        label = stringResource(R.string.folder_folder_label),
        placeholder = stringResource(R.string.folder_folder_placeholder),
        confirm = stringResource(R.string.folder_rename_confirm),
        loading = stringResource(R.string.folder_rename_loading),
        empty = stringResource(R.string.folder_folder_empty),
        duplicate = stringResource(R.string.folder_folder_duplicate),
        notDone = stringResource(R.string.folder_rename_folder_not_done),
    )

// --- Previews ----------------------------------------------------------------------------------

private fun topic(
    id: String,
    title: String,
    state: TopicState,
) = Topic(id, "f", title, state, Instant.EPOCH, Instant.EPOCH)

private val previewTopics =
    listOf(
        topic("1", "Rust", TopicState.New),
        topic("2", "Fotografia con un titolo molto lungo che va a capo su più righe", TopicState.MissionProposed),
        topic("3", "Contabilità", TopicState.Ready),
        topic("4", "Altro", TopicState.Unknown),
    )

@Composable
private fun PreviewHost(
    state: FolderUiState,
    wide: Boolean = false,
) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        FolderPaneContent(state, remember { TeachToastState() }, FolderActions(), wide = wide, onBack = {})
    }
}

@Preview(name = "Folder list", showBackground = true, heightDp = 640)
@Composable
private fun FolderListPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(FolderUiState("Lavoro", 4, previewTopics, loading = false))
    }

@Preview(name = "Folder list dark", showBackground = true, heightDp = 640)
@Composable
private fun FolderListDarkPreview() =
    TeachTheme(darkTheme = true) {
        PreviewHost(FolderUiState("Lavoro", 4, previewTopics, loading = false))
    }

@Preview(name = "Folder empty", showBackground = true, heightDp = 500)
@Composable
private fun FolderEmptyPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(FolderUiState("Hobby", 0, emptyList(), loading = false))
    }

@Preview(name = "Folder loading", showBackground = true, heightDp = 400)
@Composable
private fun FolderLoadingPreview() = TeachTheme(darkTheme = false) { PreviewHost(FolderUiState("Lavoro")) }

@Preview(name = "Folder error", showBackground = true, heightDp = 500)
@Composable
private fun FolderErrorPreview() =
    TeachTheme(darkTheme = true) {
        PreviewHost(FolderUiState("Lavoro", loading = false, loadError = LoadError.Unauthorized))
    }

@Preview(name = "Folder font 2.0", showBackground = true, heightDp = 900, fontScale = 2f)
@Composable
private fun FolderLargePreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(FolderUiState("Lavoro", 4, previewTopics, loading = false))
    }

@Preview(name = "Folder wide", showBackground = true, widthDp = 920, heightDp = 640)
@Composable
private fun FolderWidePreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(FolderUiState("Lavoro", 4, previewTopics, loading = false), wide = true)
    }
