package com.teachme.app

import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachScreenFrame
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachText

/**
 * Blocking state shown when the server's `api_version` major is higher than the app supports (api-conventions
 * section 1). It replaces every screen: there is nothing the app can safely do with a newer API. Spec: llm-key.md
 * section 10.2: TalkBack focus starts on the panel (title first), "Ricontrolla" is disabled with "Controllo…" while the
 * check runs, and its outcome is announced once.
 */
@Composable
fun UpdateRequiredScreen(
    check: UpdateCheckState,
    onRecheck: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val view = LocalView.current
    val focus = remember { FocusRequester() }
    val still = stringResource(R.string.update_required_still)
    val unreachable = stringResource(R.string.update_required_unreachable)
    // The screen is new: focus goes to the title once, not to a live region.
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    // One announcement per outcome (a new id each time), never at every recomposition.
    LaunchedEffect(check.noticeId) {
        when (check.notice) {
            UpdateNotice.StillOutdated -> view.announceForAccessibility(still)
            UpdateNotice.Unreachable -> view.announceForAccessibility(unreachable)
            null -> Unit
        }
    }
    val paneTitle = stringResource(R.string.update_required_title)
    // A new pane for TalkBack: it announces the title and starts reading from the top, over a host that is not even
    // composed any more (TeachMeApp replaces the navigation host with this screen).
    TeachScreenFrame(modifier.semantics { this.paneTitle = paneTitle }) {
        Box(Modifier.padding(TeachTheme.spacing.md), contentAlignment = Alignment.Center) {
            Column(
                Modifier.widthIn(max = TeachTheme.layout.readingMaxWidth).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
            ) {
                TeachStatusPanel(
                    StatusVariant.Warning,
                    stringResource(R.string.update_required_title),
                    modifier = Modifier.focusRequester(focus).focusable().semantics { heading() },
                    body = stringResource(R.string.update_required_body),
                    announce = false,
                )
                TeachButton(
                    stringResource(
                        if (check.checking) R.string.update_required_checking else R.string.update_required_recheck,
                    ),
                    onRecheck,
                    variant = ButtonVariant.Secondary,
                    enabled = !check.checking,
                    loading = check.checking,
                )
                if (check.notice == UpdateNotice.Unreachable) {
                    // Announced by the effect above; the text stays under the button for everyone else.
                    TeachText(
                        unreachable,
                        style = TeachTheme.typography.ui.bodyL,
                        color = TeachTheme.colors.ink,
                    )
                }
            }
        }
    }
}

@Preview(name = "Update required", showBackground = true, widthDp = 360, heightDp = 640)
@Composable
private fun UpdateRequiredPreview() =
    TeachTheme(darkTheme = false) {
        UpdateRequiredScreen(UpdateCheckState(), onRecheck = {})
    }

@Preview(name = "Update required dark", showBackground = true, widthDp = 360, heightDp = 640)
@Composable
private fun UpdateRequiredDarkPreview() =
    TeachTheme(darkTheme = true) {
        UpdateRequiredScreen(UpdateCheckState(), onRecheck = {})
    }

@Preview(name = "Update required checking", showBackground = true, widthDp = 360, heightDp = 640)
@Composable
private fun UpdateRequiredCheckingPreview() =
    TeachTheme(darkTheme = false) { UpdateRequiredScreen(UpdateCheckState(checking = true), onRecheck = {}) }

@Preview(name = "Update required unreachable dark", showBackground = true, widthDp = 360, heightDp = 640)
@Composable
private fun UpdateRequiredUnreachablePreview() =
    TeachTheme(darkTheme = true) {
        UpdateRequiredScreen(UpdateCheckState(notice = UpdateNotice.Unreachable, noticeId = 1), onRecheck = {})
    }
