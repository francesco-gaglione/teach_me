package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

private const val LARGE_FONT_TITLE_LINES = 2

/**
 * Top bar (base-components section 6): same fill as the content, no elevation. [showDivider] draws the
 * bottom hairline once content scrolls under it. [navigation] and [actions] hold [TeachIconButton]s.
 */
@Composable
fun TeachTopBar(
    title: String,
    modifier: Modifier = Modifier,
    showDivider: Boolean = false,
    titleModifier: Modifier = Modifier,
    navigation: (@Composable () -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val colors = TeachTheme.colors
    val largeFont = LocalDensity.current.fontScale > LARGE_FONT_SCALE
    Column(modifier = modifier.fillMaxWidth().background(colors.bg)) {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = TeachTheme.layout.topBarHeight)
                    .padding(horizontal = TeachTheme.spacing.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (navigation != null) {
                navigation()
            } else {
                Box(Modifier.padding(start = TeachTheme.spacing.sm))
            }
            TeachText(
                text = title,
                modifier =
                    Modifier
                        .weight(1f)
                        .then(titleModifier)
                        .padding(horizontal = TeachTheme.spacing.xs)
                        .semantics { heading() },
                style = TeachTheme.typography.ui.title2,
                maxLines = if (largeFont) LARGE_FONT_TITLE_LINES else 1,
                overflow = TextOverflow.Ellipsis,
            )
            actions()
        }
        if (showDivider) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = TeachTheme.stroke.hairline)
                    .background(colors.line),
            )
        }
    }
}

@Composable
private fun TopBarSample() {
    Column(Modifier.background(TeachTheme.colors.bg)) {
        TeachTopBar(
            title = "Impostazioni",
            navigation = { TeachIconButton(TeachIcons.ChevronLeft, "Indietro", {}, tint = TeachTheme.colors.ink) },
            actions = { TeachIconButton(TeachIcons.Settings, "Impostazioni", {}) },
            showDivider = true,
        )
        TeachTopBar(title = "Una barra senza navigazione")
    }
}

@Preview(name = "TopBar light", showBackground = true)
@Composable
private fun TopBarLightPreview() = TeachTheme(darkTheme = false) { TopBarSample() }

@Preview(name = "TopBar dark", showBackground = true)
@Composable
private fun TopBarDarkPreview() = TeachTheme(darkTheme = true) { TopBarSample() }

@Preview(name = "TopBar font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun TopBarLargeFontPreview() = TeachTheme(darkTheme = false) { TopBarSample() }
