package com.teachme.feature.home

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
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
import com.teachme.core.designsystem.components.ButtonVariant
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
import com.teachme.core.model.Folder
import com.teachme.core.ui.DeleteConfirmDialog
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyHint
import com.teachme.core.ui.LoadErrorState
import com.teachme.core.ui.NameDialogTexts
import com.teachme.core.ui.NameInputDialog
import com.teachme.core.ui.rememberNotificationPermissionRequest
import com.teachme.core.ui.rememberRowRequesters
import com.teachme.core.ui.topicCountText
import kotlinx.coroutines.launch
import java.time.Instant
import com.teachme.core.ui.R as UiR

private val BOTTOM_BUTTON_SPACE = 88.dp
private val TOAST_LIFT = 72.dp

/**
 * The folder list (home.md). A reusable pane: on compact it is the whole Home screen, on expanded it is
 * the first pane of the list-detail layout. [selectedFolderId] highlights the open folder (expanded
 * only). [onFolderRemoved] tells the host that a folder was deleted here so it can drop the selection.
 */
@Composable
fun HomePane(
    onOpenFolder: (Folder) -> Unit,
    onOpenSettings: () -> Unit,
    toast: TeachToastState,
    modifier: Modifier = Modifier,
    onOpenReview: (topicId: String?, topicTitle: String?) -> Unit = { _, _ -> },
    onOpenLesson: (String) -> Unit = {},
    onFolderRemoved: (String) -> Unit = {},
    sidebar: Boolean = false,
    showTopBar: Boolean = true,
    viewModel: HomeViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var pendingScroll by remember { mutableStateOf<String?>(null) }
    val resources = androidx.compose.ui.platform.LocalResources.current
    val currentOnRemoved by rememberUpdatedState(onFolderRemoved)

    LifecycleEventEffect(Lifecycle.Event.ON_START) { viewModel.onStart() }
    // The answer needs no handling here: the hourly check is started by the next sync of the reminder gate.
    val askNotificationPermission = rememberNotificationPermissionRequest { viewModel.onNotificationPermissionResult() }
    LaunchedEffect(viewModel) {
        viewModel.effects.collect { effect ->
            when (effect) {
                is HomeEffect.Toast -> {
                    val text =
                        when (val m = effect.message) {
                            is HomeMessage.Created -> resources.getString(R.string.home_toast_created, m.name)
                            is HomeMessage.Renamed -> resources.getString(R.string.home_toast_renamed, m.name)
                            is HomeMessage.Deleted -> resources.getString(R.string.home_toast_deleted, m.name)
                            HomeMessage.FolderGone -> resources.getString(R.string.home_toast_gone)
                        }
                    launch { toast.show(text) }
                }
                is HomeEffect.ScrollTo -> pendingScroll = effect.folderId
                is HomeEffect.FolderRemoved -> currentOnRemoved(effect.folderId)
                HomeEffect.AskNotificationPermission -> {
                    viewModel.onNotificationPermissionRequested()
                    askNotificationPermission()
                }
            }
        }
    }

    HomePaneContent(
        state = state,
        toast = toast,
        modifier = modifier,
        selectedFolderId = if (sidebar) state.selectedFolderId else null,
        sidebar = sidebar,
        showTopBar = showTopBar,
        pendingScroll = pendingScroll,
        onScrolled = { pendingScroll = null },
        actions =
            HomeActions(
                onOpenFolder = onOpenFolder,
                onOpenSettings = onOpenSettings,
                onOpenReview = onOpenReview,
                onOpenLesson = onOpenLesson,
                onRetryReview = viewModel::retryReview,
                onSelectReviewTopic = viewModel::selectReviewTopic,
                onRetry = viewModel::retry,
                onDismissRefreshError = viewModel::dismissRefreshError,
                onCreate = viewModel::openCreate,
                onRename = viewModel::openRename,
                onDelete = viewModel::openDelete,
                onDismissDialog = viewModel::dismissDialog,
                onDialogTextEdited = viewModel::onDialogTextEdited,
                onSubmitCreate = viewModel::create,
                onSubmitRename = viewModel::rename,
                onConfirmDelete = viewModel::confirmDelete,
            ),
    )
}

data class HomeActions(
    val onOpenFolder: (Folder) -> Unit = {},
    val onOpenSettings: () -> Unit = {},
    val onOpenReview: (topicId: String?, topicTitle: String?) -> Unit = { _, _ -> },
    val onSelectReviewTopic: (String?) -> Unit = {},
    val onOpenLesson: (String) -> Unit = {},
    val onRetryReview: () -> Unit = {},
    val onRetry: () -> Unit = {},
    val onDismissRefreshError: () -> Unit = {},
    val onCreate: () -> Unit = {},
    val onRename: (Folder) -> Unit = {},
    val onDelete: (Folder) -> Unit = {},
    val onDismissDialog: () -> Unit = {},
    val onDialogTextEdited: () -> Unit = {},
    val onSubmitCreate: (String) -> Unit = {},
    val onSubmitRename: (String) -> Unit = {},
    val onConfirmDelete: () -> Unit = {},
)

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun HomePaneContent(
    state: HomeUiState,
    toast: TeachToastState,
    actions: HomeActions,
    modifier: Modifier = Modifier,
    selectedFolderId: String? = null,
    sidebar: Boolean = false,
    showTopBar: Boolean = true,
    pendingScroll: String? = null,
    onScrolled: () -> Unit = {},
) {
    TeachPane(
        modifier = if (sidebar) modifier.background(TeachTheme.colors.surfaceSunken) else modifier,
        topBar = {
            if (showTopBar) {
                TeachTopBar(
                    title = stringResource(R.string.home_title),
                    actions = {
                        TeachIconButton(
                            icon = TeachIcons.Settings,
                            contentDescription = stringResource(R.string.home_open_settings),
                            onClick = actions.onOpenSettings,
                        )
                    },
                )
            }
        },
        // Stacked layout: the toast sits above the bottom "Nuova cartella" button instead of covering it.
        toast = { Box(Modifier.padding(bottom = if (sidebar) 0.dp else TOAST_LIFT)) { TeachToastHost(toast) } },
    ) {
        val folders = state.folders
        Column(Modifier.fillMaxSize()) {
            // The suggestion about the LLM key (llm-key.md 4.1): above "Cartelle" in the stacked layout; the wide layout
            // puts it under the top bar, outside this pane. Not on the empty state, where it sits under the call to action.
            if (showTopBar && !sidebar && !folders.isNullOrEmpty()) {
                LlmKeyHint(
                    LlmKeyContext.HomeHint,
                    Modifier.padding(
                        horizontal = TeachTheme.layout.screenMarginCompact,
                        vertical = TeachTheme.spacing.md,
                    ),
                )
            }
            Box(Modifier.weight(1f)) {
                when {
                    folders == null && state.loadError != null ->
                        LoadErrorState(
                            state.loadError,
                            onRetry = actions.onRetry,
                            onOpenSettings = actions.onOpenSettings,
                        )
                    folders == null -> LoadingList(state.slowLoading)
                    folders.isEmpty() -> EmptyFolders(actions.onCreate, state.refreshFailed, actions)
                    sidebar -> FolderSidebar(folders, state, actions, selectedFolderId, pendingScroll, onScrolled)
                    else -> FolderList(folders, state, actions, selectedFolderId, pendingScroll, onScrolled)
                }
            }
        }
        HomeDialogs(state, actions)
    }
}

@Composable
private fun LoadingList(slow: Boolean) {
    Column(Modifier.fillMaxSize().padding(TeachTheme.layout.screenMarginCompact)) {
        SectionHeading()
        TeachListSkeleton(
            description = stringResource(R.string.home_loading_folders),
            stateText = stringResource(UiR.string.ui_loading_state),
            slowText = if (slow) stringResource(UiR.string.ui_still_loading) else null,
        )
    }
}

@Composable
private fun SectionHeading() {
    TeachText(
        stringResource(R.string.home_section_folders),
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
private fun EmptyFolders(
    onCreate: () -> Unit,
    refreshFailed: Boolean,
    actions: HomeActions,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(TeachTheme.layout.screenMarginCompact),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (refreshFailed) {
            TeachErrorBanner(
                stringResource(R.string.home_refresh_failed),
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
                stringResource(R.string.home_empty_title),
                modifier = Modifier.semantics { heading() },
                style = TeachTheme.typography.ui.title1,
                textAlign = TextAlign.Center,
            )
            TeachText(
                stringResource(R.string.home_empty_body),
                textAlign = TextAlign.Center,
                style = TeachTheme.typography.ui.bodyL,
                color = TeachTheme.colors.inkSoft,
            )
            TeachButton(stringResource(R.string.home_empty_cta), onCreate, size = ButtonSize.Large)
            // Under the call to action, never above it (llm-key.md section 4.1).
            LlmKeyHint(LlmKeyContext.HomeHint, Modifier.padding(top = TeachTheme.spacing.md))
        }
    }
}

/** Expanded layout, left pane (home.md, Expanded): header, plain rows with selection, secondary button. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun FolderSidebar(
    folders: List<Folder>,
    state: HomeUiState,
    actions: HomeActions,
    selectedFolderId: String?,
    pendingScroll: String?,
    onScrolled: () -> Unit,
) {
    val requesters = rememberRowRequesters()
    var menuFor by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(pendingScroll, folders) {
        val id = pendingScroll ?: return@LaunchedEffect
        if (folders.any { it.id == id }) {
            requesters.of(id).bringIntoView()
            onScrolled()
        }
    }
    val paneTitle = stringResource(R.string.home_section_folders)
    Column(Modifier.fillMaxSize().semantics { this.paneTitle = paneTitle }) {
        ReviewEntry(
            state.review,
            state.reviewTopicId,
            actions,
            Modifier.padding(start = TeachTheme.spacing.md, end = TeachTheme.spacing.md, top = TeachTheme.spacing.md),
        )
        ResumeLessonCard(
            state.resume,
            actions.onOpenLesson,
            Modifier.padding(horizontal = TeachTheme.spacing.md),
        )
        Box(
            Modifier
                .fillMaxWidth()
                .heightIn(
                    min = TeachTheme.layout.paneHeaderHeight,
                ).padding(horizontal = TeachTheme.spacing.md),
            contentAlignment = Alignment.CenterStart,
        ) {
            TeachText(paneTitle, modifier = Modifier.semantics { heading() }, style = TeachTheme.typography.ui.title2)
        }
        if (state.refreshFailed) {
            TeachErrorBanner(
                stringResource(R.string.home_refresh_failed),
                stringResource(UiR.string.ui_load_retry),
                actions.onRetry,
                stringResource(UiR.string.ui_load_dismiss),
                actions.onDismissRefreshError,
                modifier = Modifier.padding(horizontal = TeachTheme.spacing.md),
            )
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            folders.forEachIndexed { index, folder ->
                if (index > 0) TeachRowDivider()
                FolderRow(
                    folder = folder,
                    selected = folder.id == selectedFolderId,
                    menuOpen = menuFor == folder.id,
                    onMenuOpen = { menuFor = folder.id },
                    onMenuClose = { menuFor = null },
                    actions = actions,
                    modifier =
                        Modifier.bringIntoViewRequester(
                            requesters.of(folder.id),
                        ),
                )
            }
        }
        TeachButton(
            stringResource(R.string.home_new_folder),
            actions.onCreate,
            modifier = Modifier.padding(TeachTheme.spacing.md).fillMaxWidth(),
            variant = ButtonVariant.Secondary,
            leadingIcon = TeachIcons.Plus,
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun FolderList(
    folders: List<Folder>,
    state: HomeUiState,
    actions: HomeActions,
    selectedFolderId: String?,
    pendingScroll: String?,
    onScrolled: () -> Unit,
) {
    val requesters = rememberRowRequesters()
    var menuFor by rememberSaveable { mutableStateOf<String?>(null) }
    LaunchedEffect(pendingScroll, folders) {
        val id = pendingScroll ?: return@LaunchedEffect
        if (folders.any { it.id == id }) {
            requesters.of(id).bringIntoView()
            onScrolled()
        }
    }
    val bottomSpace = BOTTOM_BUTTON_SPACE * LocalDensity.current.fontScale
    Box(Modifier.fillMaxSize()) {
        Column(
            modifier =
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = TeachTheme.layout.screenMarginCompact)
                    .padding(bottom = bottomSpace)
                    .widthIn(max = TeachTheme.layout.contentMaxWidth),
        ) {
            if (state.refreshFailed) {
                TeachErrorBanner(
                    stringResource(R.string.home_refresh_failed),
                    stringResource(UiR.string.ui_load_retry),
                    actions.onRetry,
                    stringResource(UiR.string.ui_load_dismiss),
                    actions.onDismissRefreshError,
                    modifier = Modifier.padding(top = TeachTheme.spacing.md),
                )
            }
            ReviewEntry(state.review, state.reviewTopicId, actions, Modifier.padding(top = TeachTheme.spacing.md))
            ResumeLessonCard(state.resume, actions.onOpenLesson)
            SectionHeading()
            TeachCard(Modifier.fillMaxWidth()) {
                Column {
                    folders.forEachIndexed { index, folder ->
                        if (index > 0) TeachRowDivider()
                        FolderRow(
                            folder = folder,
                            selected = folder.id == selectedFolderId,
                            menuOpen = menuFor == folder.id,
                            onMenuOpen = { menuFor = folder.id },
                            onMenuClose = { menuFor = null },
                            actions = actions,
                            modifier =
                                Modifier.bringIntoViewRequester(
                                    requesters.of(folder.id),
                                ),
                        )
                    }
                }
            }
        }
        TeachButton(
            stringResource(R.string.home_new_folder),
            actions.onCreate,
            modifier =
                Modifier
                    .align(
                        Alignment.BottomCenter,
                    ).padding(TeachTheme.layout.screenMarginCompact)
                    .fillMaxWidth()
                    .widthIn(max = 480.dp),
            size = ButtonSize.Large,
            // One primary per screen: while the review card has its own, "Nuova cartella" steps down.
            variant = if (state.review is HomeReview.Due) ButtonVariant.Secondary else ButtonVariant.Primary,
            leadingIcon = TeachIcons.Plus,
        )
    }
}

@Composable
private fun FolderRow(
    folder: Folder,
    selected: Boolean,
    menuOpen: Boolean,
    onMenuOpen: () -> Unit,
    onMenuClose: () -> Unit,
    actions: HomeActions,
    modifier: Modifier = Modifier,
) {
    val count = topicCountText(folder.topicCount)
    val renameLabel = stringResource(UiR.string.ui_action_rename)
    val deleteLabel = stringResource(UiR.string.ui_action_delete)
    Box(modifier) {
        TeachListRow(
            title = folder.name,
            onClick = { actions.onOpenFolder(folder) },
            contentDescription = stringResource(R.string.home_row_description, folder.name, count),
            leadingIcon = TeachIcons.Folder,
            subtitle = count,
            selected = selected,
            onLongClick = onMenuOpen,
            customActions =
                listOf(
                    CustomAccessibilityAction(renameLabel) {
                        actions.onRename(folder)
                        true
                    },
                    CustomAccessibilityAction(deleteLabel) {
                        actions.onDelete(folder)
                        true
                    },
                ),
            trailing = {
                TeachIconButton(
                    icon = TeachIcons.MoreVertical,
                    contentDescription = stringResource(UiR.string.ui_more_actions_for, folder.name),
                    onClick = onMenuOpen,
                )
            },
        )
        TeachActionMenu(
            expanded = menuOpen,
            onDismiss = onMenuClose,
            title = stringResource(UiR.string.ui_actions_for, folder.name),
            items =
                listOf(
                    ActionMenuItem(renameLabel, TeachIcons.Pencil, { actions.onRename(folder) }),
                    ActionMenuItem(deleteLabel, TeachIcons.Trash, { actions.onDelete(folder) }, destructive = true),
                ),
        )
    }
}

@Composable
private fun HomeDialogs(
    state: HomeUiState,
    actions: HomeActions,
) {
    when (val dialog = state.dialog) {
        null -> Unit
        is HomeDialog.Create ->
            NameInputDialog(
                texts = nameTexts(rename = false),
                maxLength = FOLDER_NAME_MAX,
                ui = dialog.ui,
                onConfirm = actions.onSubmitCreate,
                onDismiss = actions.onDismissDialog,
                onTextEdited = actions.onDialogTextEdited,
                onOpenSettings = actions.onOpenSettings,
            )
        is HomeDialog.Rename ->
            NameInputDialog(
                texts = nameTexts(rename = true),
                maxLength = FOLDER_NAME_MAX,
                ui = dialog.ui,
                onConfirm = actions.onSubmitRename,
                onDismiss = actions.onDismissDialog,
                onTextEdited = actions.onDialogTextEdited,
                onOpenSettings = actions.onOpenSettings,
                initialText = dialog.folder.name,
            )
        is HomeDialog.Delete -> {
            val count = dialog.folder.topicCount
            DeleteConfirmDialog(
                title = stringResource(R.string.home_delete_title, dialog.folder.name),
                body =
                    when (count) {
                        0 -> stringResource(R.string.home_delete_body_empty)
                        1 -> stringResource(R.string.home_delete_body_one)
                        else -> stringResource(R.string.home_delete_body_many, count)
                    },
                confirmLabel =
                    stringResource(
                        when (count) {
                            0 -> R.string.home_delete_confirm_empty
                            1 -> R.string.home_delete_confirm_one
                            else -> R.string.home_delete_confirm_many
                        },
                    ),
                loadingLabel = stringResource(R.string.home_delete_loading),
                notDone = stringResource(R.string.home_delete_not_done),
                ui = dialog.ui,
                onConfirm = actions.onConfirmDelete,
                onDismiss = actions.onDismissDialog,
                onOpenSettings = actions.onOpenSettings,
            )
        }
    }
}

@Composable
private fun nameTexts(rename: Boolean) =
    NameDialogTexts(
        title = stringResource(if (rename) R.string.home_rename_title else R.string.home_create_title),
        label = stringResource(R.string.home_create_label),
        placeholder = stringResource(R.string.home_create_placeholder),
        confirm = stringResource(if (rename) R.string.home_rename_confirm else R.string.home_create_confirm),
        loading = stringResource(if (rename) R.string.home_rename_loading else R.string.home_create_loading),
        empty = stringResource(R.string.home_name_empty),
        duplicate = stringResource(R.string.home_name_duplicate),
        notDone = stringResource(if (rename) R.string.home_rename_not_done else R.string.home_create_not_done),
    )

// --- Previews ----------------------------------------------------------------------------------

private fun sample(
    id: String,
    name: String,
    count: Int,
) = Folder(id, name, 0, count, Instant.EPOCH, Instant.EPOCH)

@Composable
private fun PreviewHost(state: HomeUiState) {
    Box(Modifier.background(TeachTheme.colors.bg)) {
        HomePaneContent(state, remember { TeachToastState() }, HomeActions())
    }
}

private val previewFolders =
    listOf(
        sample("1", "Lavoro", 3),
        sample("2", "Hobby", 0),
        sample("3", "Una cartella con un nome piuttosto lungo che va a capo", 1),
    )

@Preview(name = "Home list", showBackground = true, heightDp = 640)
@Composable
private fun HomeListPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(HomeUiState(previewFolders, loading = false))
    }

@Preview(name = "Home list dark", showBackground = true, heightDp = 640)
@Composable
private fun HomeListDarkPreview() =
    TeachTheme(darkTheme = true) {
        PreviewHost(HomeUiState(previewFolders, loading = false))
    }

@Preview(name = "Home list font 2.0", showBackground = true, heightDp = 800, fontScale = 2f)
@Composable
private fun HomeListLargePreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(HomeUiState(previewFolders, loading = false))
    }

@Preview(name = "Home loading", showBackground = true, heightDp = 400)
@Composable
private fun HomeLoadingPreview() = TeachTheme(darkTheme = false) { PreviewHost(HomeUiState(slowLoading = true)) }

@Preview(name = "Home empty", showBackground = true, heightDp = 500)
@Composable
private fun HomeEmptyPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(HomeUiState(emptyList(), loading = false))
    }

@Preview(name = "Home error", showBackground = true, heightDp = 500)
@Composable
private fun HomeErrorPreview() =
    TeachTheme(darkTheme = true) {
        PreviewHost(HomeUiState(loading = false, loadError = com.teachme.core.model.LoadError.Network))
    }

@Preview(name = "Home refresh banner", showBackground = true, heightDp = 640)
@Composable
private fun HomeBannerPreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(HomeUiState(previewFolders, loading = false, refreshFailed = true))
    }

@Preview(name = "Home delete dialog", showBackground = true, heightDp = 640)
@Composable
private fun HomeDeletePreview() =
    TeachTheme(darkTheme = false) {
        PreviewHost(HomeUiState(previewFolders, loading = false, dialog = HomeDialog.Delete(previewFolders[0])))
    }
