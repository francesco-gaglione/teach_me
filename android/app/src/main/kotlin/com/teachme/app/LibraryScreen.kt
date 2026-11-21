package com.teachme.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachScreenFrame
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.TeachToastState
import com.teachme.core.designsystem.components.TeachTopBar
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashKind
import com.teachme.core.model.Topic
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyHint
import com.teachme.feature.folder.FolderPane
import com.teachme.feature.home.HomeMessage
import com.teachme.feature.home.HomePane
import com.teachme.feature.home.HomeViewModel

/**
 * Expanded layout (design-system.md 5.1): app bar across the window, then folders (fixed width) and the
 * topics of the selected folder. The selection lives in [homeViewModel]; the topic itself opens as a
 * full-window route on top of this screen.
 */
@Composable
fun LibraryScreen(
    homeViewModel: HomeViewModel,
    initialFolderId: String?,
    onOpenTopic: (Topic) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenReview: (topicId: String?, topicTitle: String?) -> Unit,
    onOpenLesson: (String) -> Unit,
) {
    val state by homeViewModel.state.collectAsStateWithLifecycle()
    val leftToast = remember { TeachToastState() }
    val rightToast = remember { TeachToastState() }
    // Coming from a Folder route (e.g. rotation from compact): that folder is the selection.
    LaunchedEffect(initialFolderId) { initialFolderId?.let(homeViewModel::select) }

    TeachScreenFrame {
        Column(Modifier.fillMaxSize()) {
            TeachTopBar(
                title = stringResource(R.string.app_name),
                actions = {
                    TeachIconButton(
                        icon = TeachIcons.Settings,
                        contentDescription = stringResource(R.string.home_open_settings),
                        onClick = onOpenSettings,
                    )
                },
            )
            val folders = state.folders
            // Wide layout: the key suggestion is a band under the top bar (llm-key.md 4.1); the empty state has its own.
            if (!folders.isNullOrEmpty()) {
                LlmKeyHint(
                    LlmKeyContext.HomeHint,
                    Modifier.padding(
                        horizontal = TeachTheme.layout.screenMarginExpanded,
                        vertical = TeachTheme.spacing.xs,
                    ),
                )
            }
            if (folders != null && folders.isEmpty()) {
                // No folders: one centered column with the empty state, no panes yet.
                HomePane(
                    onOpenFolder = {},
                    onOpenSettings = onOpenSettings,
                    onOpenReview = onOpenReview,
                    onOpenLesson = onOpenLesson,
                    toast = leftToast,
                    showTopBar = false,
                    viewModel = homeViewModel,
                )
            } else {
                Row(Modifier.fillMaxSize()) {
                    HomePane(
                        onOpenFolder = { homeViewModel.select(it.id) },
                        onOpenSettings = onOpenSettings,
                        onOpenReview = onOpenReview,
                        onOpenLesson = onOpenLesson,
                        toast = leftToast,
                        sidebar = true,
                        showTopBar = false,
                        viewModel = homeViewModel,
                        modifier = Modifier.width(TeachTheme.layout.listPaneWidth).fillMaxHeight(),
                    )
                    Box(Modifier.width(TeachTheme.stroke.hairline).fillMaxHeight().background(TeachTheme.colors.line))
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        val selected = folders?.firstOrNull { it.id == state.selectedFolderId }
                        if (selected != null) {
                            key(selected.id) {
                                FolderPane(
                                    folderId = selected.id,
                                    folderName = selected.name,
                                    onOpenTopic = onOpenTopic,
                                    onLeave = { flash ->
                                        onFolderLeft(homeViewModel, selected.id, selected.name, flash)
                                    },
                                    onOpenSettings = onOpenSettings,
                                    toast = rightToast,
                                    onChanged = homeViewModel::reload,
                                    wide = true,
                                    modifier = Modifier.fillMaxSize(),
                                )
                            }
                        } else if (folders != null) {
                            TeachText(
                                stringResource(R.string.library_pick_folder),
                                modifier = Modifier.align(Alignment.Center).padding(32.dp),
                                style = TeachTheme.typography.ui.bodyL,
                                color = TeachTheme.colors.inkSoft,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun onFolderLeft(
    homeViewModel: HomeViewModel,
    id: String,
    name: String,
    flash: Flash,
) {
    homeViewModel.folderRemoved(
        id,
        if (flash.kind == FlashKind.FolderDeleted) HomeMessage.Deleted(flash.name ?: name) else HomeMessage.FolderGone,
    )
}
