package com.teachme.sdui.render

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R

/**
 * The neutral row for a block the app cannot draw (docs/design/components/missing_component.md section 2).
 * Never a warning or error color: it is not the learner's fault and not a danger.
 */
@Composable
internal fun UnavailableBlockView() {
    val text = stringResource(R.string.sdui_unavailable)
    SelectionContainer {
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .padding(vertical = TeachTheme.spacing.xs)
                    .semantics(mergeDescendants = true) { contentDescription = text },
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TeachIcon(
                TeachIcons.Puzzle,
                contentDescription = null,
                tint = TeachTheme.colors.inkMuted,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(text, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        }
    }
}
