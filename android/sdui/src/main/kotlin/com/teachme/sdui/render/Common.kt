package com.teachme.sdui.render

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.sdui.inline.InlineMarkdown

/** True when a hardware keyboard is attached: the shortcut hints are shown (docs/design _exercises.md section 6). */
@Composable
fun rememberHardwareKeyboard(): Boolean {
    val configuration = LocalConfiguration.current
    return configuration.keyboard != android.content.res.Configuration.KEYBOARD_NOKEYS &&
        configuration.hardKeyboardHidden == android.content.res.Configuration.HARDKEYBOARDHIDDEN_NO
}

/** Text a screen reader should get for a markdown field: markup removed, terms spelled out. */
@Composable
internal fun plain(markdown: String): String {
    val env = LocalSdui.current
    return InlineMarkdown.plainText(markdown) { env.document.glossaryEntry(it)?.term }
}

/**
 * A card that is one single button target: the whole surface is tappable, with a pressed fill and the
 * focus ring. [enabled] false draws it flat (no ring, no click).
 */
@Composable
internal fun TapCard(
    onClick: (() -> Unit)?,
    description: String,
    onClickLabel: String?,
    modifier: Modifier = Modifier,
    stateText: String? = null,
    content: @Composable () -> Unit,
) {
    val colors = TeachTheme.colors
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val fill = if (state.pressed && onClick != null) colors.surfacePressed else colors.surface
    Box(
        modifier =
            modifier
                .fillMaxWidth()
                .focusRing(state.focused && onClick != null, shape)
                .clip(shape)
                .background(fill, shape)
                .border(BorderStroke(TeachTheme.stroke.hairline, colors.line), shape)
                .semantics(mergeDescendants = true) {
                    contentDescription = description
                    if (stateText != null) stateDescription = stateText
                }.then(
                    if (onClick != null) {
                        Modifier.clickable(
                            interactionSource = state.source,
                            indication = null,
                            role = Role.Button,
                            onClickLabel = onClickLabel,
                            onClick = onClick,
                        )
                    } else {
                        Modifier
                    },
                ).padding(TeachTheme.spacing.md),
    ) {
        content()
    }
}

@Composable
internal fun Hairline(
    modifier: Modifier = Modifier,
    color: Color = TeachTheme.colors.line,
) {
    Box(
        modifier
            .fillMaxWidth()
            .padding(
                vertical = TeachTheme.spacing.xxs,
            ).background(color)
            .padding(top = TeachTheme.stroke.hairline),
    )
}
