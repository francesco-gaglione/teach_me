package com.teachme.core.designsystem.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.R
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme

private val DISCLOSURE_MIN_HEIGHT = 48.dp

/**
 * Disclosure (base-components section 13): a 48 dp row with a chevron and a title that shows or hides
 * [content] right below it. It reads as a button with an "Aperto" / "Chiuso" state; the content comes
 * next in reading order. [title] is the visible text; [description] overrides what screen readers say.
 */
@Composable
fun TeachDisclosure(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    description: String? = null,
    content: @Composable () -> Unit,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.md
    val stateText = stringResource(if (expanded) R.string.teach_state_open else R.string.teach_state_closed)
    Column(modifier.fillMaxWidth()) {
        Row(
            Modifier
                .fillMaxWidth()
                .focusRing(state.focused, shape)
                .background(if (state.pressed) TeachTheme.colors.surfacePressed else Color.Transparent, shape)
                .semantics(mergeDescendants = true) {
                    stateDescription = stateText
                    if (description != null) contentDescription = description
                }.clickable(
                    interactionSource = state.source,
                    indication = null,
                    role = Role.Button,
                    onClick = onToggle,
                ).heightIn(min = DISCLOSURE_MIN_HEIGHT),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(
                if (expanded) TeachIcons.ChevronDown else TeachIcons.ChevronRight,
                contentDescription = null,
                tint = TeachTheme.colors.inkSoft,
            )
            TeachText(
                title,
                Modifier.weight(1f).padding(vertical = TeachTheme.spacing.xs),
                style = TeachTheme.typography.ui.label,
                color = TeachTheme.colors.ink,
            )
        }
        if (expanded) content()
    }
}

@Composable
private fun DisclosureSample() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        TeachDisclosure("Versioni precedenti (2)", expanded = false, onToggle = {}) {}
        TeachDisclosure("Versioni precedenti (2)", expanded = true, onToggle = {}) {
            TeachText("Contenuto", style = TeachTheme.typography.ui.body)
        }
    }
}

@Preview(name = "Disclosure light", showBackground = true)
@Composable
private fun DisclosureLightPreview() = TeachTheme(darkTheme = false) { DisclosureSample() }

@Preview(name = "Disclosure dark", showBackground = true)
@Composable
private fun DisclosureDarkPreview() = TeachTheme(darkTheme = true) { DisclosureSample() }
