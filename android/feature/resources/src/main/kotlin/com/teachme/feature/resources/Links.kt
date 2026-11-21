package com.teachme.feature.resources

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.res.stringResource
import androidx.core.net.toUri
import com.teachme.core.designsystem.components.TeachToastState
import kotlinx.coroutines.launch

/**
 * Opens an `https` URL in a Custom Tab (the user keeps the context, Back returns to the app); without a browser that
 * supports tabs it falls back to `ACTION_VIEW`. Other schemes are ignored (resources.md 8.3).
 */
internal fun openInCustomTab(
    context: Context,
    url: String,
) {
    val safe = url.httpsOrNull() ?: return
    try {
        CustomTabsIntent.Builder().build().launchUrl(context, safe.toUri())
    } catch (_: ActivityNotFoundException) {
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, safe.toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (_: ActivityNotFoundException) {
            // No browser installed: nothing sensible to do.
        }
    }
}

/** Copies a link to the clipboard; on Android < 13 (no system confirmation) it also shows a toast. */
@Composable
internal fun rememberCopyLink(toast: TeachToastState?): (String) -> Unit {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val copied = stringResource(R.string.res_link_copied)
    return { text ->
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(null, text)))
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) toast?.show(copied)
        }
    }
}
