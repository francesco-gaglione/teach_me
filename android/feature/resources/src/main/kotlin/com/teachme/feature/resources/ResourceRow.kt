package com.teachme.feature.resources

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ActionMenuItem
import com.teachme.core.designsystem.components.TeachActionMenu
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.model.Resource
import com.teachme.core.model.SourceType

/** Minimum height of a source row (resources.md 8.4): the whole row is one target. */
private val ROW_MIN_HEIGHT = 96.dp

internal data class SourceTypeLook(
    @StringRes val label: Int,
    @DrawableRes val icon: Int,
)

/** Word and icon of a source type (resources.md 8.1); a type this app does not know reads "Fonte". */
internal fun SourceType.look(): SourceTypeLook =
    when (this) {
        SourceType.Book -> SourceTypeLook(R.string.res_type_book, TeachIcons.Book)
        SourceType.Paper -> SourceTypeLook(R.string.res_type_paper, TeachIcons.FlaskConical)
        SourceType.Article -> SourceTypeLook(R.string.res_type_article, TeachIcons.Newspaper)
        SourceType.Documentation -> SourceTypeLook(R.string.res_type_documentation, TeachIcons.FileText)
        SourceType.Video -> SourceTypeLook(R.string.res_type_video, TeachIcons.PlayCircle)
        SourceType.Course -> SourceTypeLook(R.string.res_type_course, TeachIcons.GraduationCap)
        SourceType.Standard -> SourceTypeLook(R.string.res_type_standard, TeachIcons.FileText)
        SourceType.Pdf -> SourceTypeLook(R.string.res_type_pdf, TeachIcons.FileText)
        SourceType.Forum -> SourceTypeLook(R.string.res_type_forum, TeachIcons.MessageSquare)
        SourceType.SocialGroup -> SourceTypeLook(R.string.res_type_community, TeachIcons.Users)
        SourceType.LiveClass -> SourceTypeLook(R.string.res_type_live, TeachIcons.MapPin)
        SourceType.LocalGroup -> SourceTypeLook(R.string.res_type_local, TeachIcons.MapPin)
        SourceType.Other, SourceType.Unknown -> SourceTypeLook(R.string.res_type_other, TeachIcons.Link)
    }

/**
 * One source (resources.md section 8): type and domain above the title, then what it covers and "Usala per".
 * A source with an `https` link is one target (tap opens it, long press or the menu copies it); a source without one
 * is plain text.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ResourceRow(
    resource: Resource,
    isNew: Boolean,
    onOpen: (String) -> Unit,
    onCopy: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = TeachTheme.colors
    val look = resource.sourceType.look()
    val typeWord = stringResource(look.label)
    val domain = resource.displayDomain()
    val link = resource.openableUrl()
    val state = rememberInteractionState()
    var menu by rememberSaveable { mutableStateOf(false) }

    val base =
        if (domain != null) {
            stringResource(
                R.string.res_row_description,
                typeWord,
                resource.title,
                domain,
                resource.annotation,
                resource.useFor,
            )
        } else {
            stringResource(
                R.string.res_row_description_no_domain,
                typeWord,
                resource.title,
                resource.annotation,
                resource.useFor,
            )
        }
    val newPrefix = if (isNew) stringResource(R.string.res_row_new) + " " else ""
    val description = newPrefix + base + if (link != null) " " + stringResource(R.string.res_row_external) else ""
    val openLabel = stringResource(R.string.res_row_open)
    val copyLabel = stringResource(R.string.res_row_copy)

    val interaction =
        if (link != null) {
            Modifier
                .focusRing(state.focused, TeachTheme.shapes.md)
                .background(
                    if (state.pressed) colors.surfacePressed else androidx.compose.ui.graphics.Color.Transparent,
                ).combinedClickable(
                    interactionSource = state.source,
                    indication = null,
                    role = Role.Button,
                    onClickLabel = openLabel,
                    onClick = { onOpen(link) },
                    onLongClick = { menu = true },
                ).semantics(mergeDescendants = true) {
                    contentDescription = description
                    customActions =
                        listOf(
                            CustomAccessibilityAction(copyLabel) {
                                onCopy(link)
                                true
                            },
                            CustomAccessibilityAction(openLabel) {
                                onOpen(link)
                                true
                            },
                        )
                }
        } else {
            Modifier.semantics(mergeDescendants = true) { contentDescription = description }
        }

    Box(modifier.fillMaxWidth()) {
        Column(
            Modifier
                .fillMaxWidth()
                .then(interaction)
                .heightIn(min = ROW_MIN_HEIGHT)
                .padding(TeachTheme.spacing.md),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            Row(
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(
                    look.icon,
                    contentDescription = null,
                    tint = colors.inkSoft,
                    size = TeachTheme.layout.iconSizeSmall,
                )
                TeachText(
                    if (domain != null) "$typeWord · $domain" else typeWord,
                    Modifier.weight(1f),
                    style = TeachTheme.typography.ui.bodyS,
                    color = colors.inkSoft,
                )
                if (isNew) NewPill()
                if (link != null) {
                    TeachIcon(
                        TeachIcons.ExternalLink,
                        contentDescription = null,
                        tint = colors.inkSoft,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                }
            }
            TeachText(resource.title, style = TeachTheme.typography.ui.title3, color = colors.ink)
            if (resource.annotation.isNotBlank()) {
                TeachText(resource.annotation, style = TeachTheme.typography.ui.body, color = colors.ink)
            }
            if (resource.useFor.isNotBlank()) {
                val label = stringResource(R.string.res_use_for)
                TeachText(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(label) }
                        append(" ")
                        append(resource.useFor)
                    },
                    style = TeachTheme.typography.ui.body,
                    color = colors.ink,
                )
            }
            // A URL that is not https is not opened: it is shown as text the user can select (8.2).
            val plainUrl = resource.url?.takeIf { link == null && it.isNotBlank() }
            if (plainUrl != null) {
                SelectionContainer {
                    TeachText(plainUrl, style = TeachTheme.typography.ui.bodyS, color = colors.inkSoft)
                }
            }
        }
        if (link != null) {
            Box(Modifier.align(Alignment.TopEnd)) {
                TeachActionMenu(
                    menu,
                    { menu = false },
                    stringResource(R.string.res_row_menu, resource.title),
                    listOf(
                        ActionMenuItem(copyLabel, TeachIcons.Copy, {
                            menu = false
                            onCopy(link)
                        }),
                        ActionMenuItem(openLabel, TeachIcons.ExternalLink, {
                            menu = false
                            onOpen(link)
                        }),
                    ),
                )
            }
        }
    }
}

/** "Nuova": only on sources that the last update added, until the tab is left (resources.md 6.2). */
@Composable
private fun NewPill() {
    val colors = TeachTheme.colors
    TeachText(
        stringResource(R.string.res_new),
        Modifier
            .background(colors.infoSoft, TeachTheme.shapes.sm)
            .padding(horizontal = TeachTheme.spacing.xs, vertical = TeachTheme.spacing.xxs),
        style = TeachTheme.typography.ui.labelS,
        color = colors.info,
    )
}

@Composable
private fun RowSamples() {
    Column(Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md)) {
        PreviewData.resources.forEachIndexed { index, resource ->
            ResourceRow(resource, isNew = index == 0, onOpen = {}, onCopy = {})
        }
    }
}

@Preview(name = "Row light", showBackground = true, widthDp = 640)
@Composable
private fun RowLightPreview() = TeachTheme(darkTheme = false) { RowSamples() }

@Preview(name = "Row dark", showBackground = true, widthDp = 640)
@Composable
private fun RowDarkPreview() = TeachTheme(darkTheme = true) { RowSamples() }

@Preview(name = "Row compact font 2.0", showBackground = true, widthDp = 360, fontScale = 2f)
@Composable
private fun RowLargePreview() = TeachTheme(darkTheme = false) { RowSamples() }
