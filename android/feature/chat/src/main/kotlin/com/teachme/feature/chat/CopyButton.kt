package com.teachme.feature.chat

import android.content.ClipData
import android.os.Build
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIconButton
import com.teachme.core.designsystem.components.TeachToastState
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val COPIED_FEEDBACK_MS = 1_500L

/** Copies plain text to the clipboard; on Android < 13 (no system confirmation) also shows a toast. */
@Composable
internal fun rememberCopyAction(toast: TeachToastState?): (String) -> Unit {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val toastText = stringResource(R.string.chat_copied_toast)
    return { text ->
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(null, text)))
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) toast?.show(toastText)
        }
    }
}

/** Icon button that copies [text] and shows a check for 1.5 s (chat.md 4.1). */
@Composable
internal fun CopyButton(
    text: () -> String,
    description: String,
    copy: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) {
        if (copied) {
            delay(COPIED_FEEDBACK_MS)
            copied = false
        }
    }
    TeachIconButton(
        icon = if (copied) TeachIcons.Check else TeachIcons.Copy,
        contentDescription = if (copied) stringResource(R.string.chat_copied) else description,
        onClick = {
            copy(text())
            copied = true
        },
        modifier = modifier.semantics { if (copied) liveRegion = LiveRegionMode.Polite },
        tint = TeachTheme.colors.inkMuted,
    )
}
