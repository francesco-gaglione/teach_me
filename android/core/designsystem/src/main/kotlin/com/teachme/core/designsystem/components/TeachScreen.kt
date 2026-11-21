package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import com.teachme.core.designsystem.TeachTheme

/**
 * Screen frame: app background, system insets, top bar slot, content, toast slot pinned to the
 * bottom. The content is not padded: screens apply their own margins (layout.screenMarginCompact).
 */
@Composable
fun TeachScreen(
    modifier: Modifier = Modifier,
    topBar: @Composable () -> Unit = {},
    toast: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    Box(
        modifier =
            modifier
                .fillMaxSize()
                .background(TeachTheme.colors.bg),
    ) {
        ProvideContent(TeachTheme.colors.ink, TeachTheme.typography.ui.bodyL) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                topBar()
                Box(Modifier.weight(1f)) { content() }
            }
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .windowInsetsPadding(WindowInsets.safeDrawing),
            ) { toast() }
        }
    }
}

/** App background plus system insets, without top bar or toast: the host of one or more [TeachPane]s. */
@Composable
fun TeachScreenFrame(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Box(modifier.fillMaxSize().background(TeachTheme.colors.bg)) {
        ProvideContent(TeachTheme.colors.ink, TeachTheme.typography.ui.bodyL) {
            Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) { content() }
        }
    }
}

/**
 * One column of content: top bar, body, and a toast pinned to the bottom of the pane. Used both as a
 * full screen (inside [TeachScreenFrame]) and as a pane of a multi-pane layout.
 */
@Composable
fun TeachPane(
    modifier: Modifier = Modifier,
    topBar: @Composable () -> Unit = {},
    toast: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    Box(modifier) {
        Column(Modifier.fillMaxSize()) {
            topBar()
            Box(Modifier.weight(1f)) { content() }
        }
        Box(Modifier.align(Alignment.BottomCenter)) { toast() }
    }
}
