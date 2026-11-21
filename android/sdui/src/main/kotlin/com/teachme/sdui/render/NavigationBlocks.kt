package com.teachme.sdui.render

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.sdui.R
import com.teachme.sdui.inline.InlineText
import com.teachme.sdui.model.AskAgent
import com.teachme.sdui.model.LessonLink
import com.teachme.sdui.model.PrimarySource

/** Host of a url without the `www.` prefix, for display. Null when the url cannot be parsed. */
internal fun displayHost(url: String): String? =
    runCatching { java.net.URI(url).host }
        .getOrNull()
        ?.removePrefix("www.")
        ?.takeIf { it.isNotBlank() }

/** `primary_source` (docs/design/components/primary_source.md): where to dig deeper, one big tap target. */
@Composable
internal fun PrimarySourceBlock(block: PrimarySource) {
    val env = LocalSdui.current
    val source = remember(block.source, env.document) { env.document.source(block.source) } ?: return
    val colors = TeachTheme.colors
    val url = source.url
    val host = url?.let(::displayHost)
    val opens = url != null && url.startsWith("https://", ignoreCase = true)
    val label = stringResource(R.string.sdui_primary_source_label)
    val description =
        if (url == null) {
            // The learner's own PDF (primary_source.md): read-only, no link and no "opens the browser" hint.
            stringResource(R.string.sdui_primary_source_pdf_a11y, label, source.title, plain(block.why))
        } else {
            stringResource(R.string.sdui_primary_source_a11y, label, source.title, host.orEmpty(), plain(block.why))
        }
    TapCard(
        onClick = if (opens && url != null) ({ env.host.openUrl(url) }) else null,
        description = description,
        onClickLabel = stringResource(R.string.sdui_primary_source_open),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(
                    if (url == null) TeachIcons.FileText else TeachIcons.BookOpen,
                    contentDescription = null,
                    tint = colors.knowledge,
                )
                TeachText(label, Modifier.weight(1f), style = TeachTheme.typography.ui.label, color = colors.knowledge)
                if (opens) {
                    TeachIcon(
                        TeachIcons.ExternalLink,
                        contentDescription = null,
                        tint = colors.inkSoft,
                        size = TeachTheme.layout.iconSizeSmall,
                    )
                }
            }
            TeachText(source.title, style = TeachTheme.typography.ui.title3, color = colors.ink)
            TeachText(
                text =
                    if (url == null) {
                        stringResource(R.string.sdui_pdf_label)
                    } else if (opens) {
                        host.orEmpty()
                    } else {
                        url
                    },
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
            )
            InlineText(
                markdown = block.why,
                style = TeachTheme.typography.reading.body,
                interactive = false,
                lang = block.lang,
            )
        }
    }
}

/** `lesson_link` (docs/design/components/lesson_link.md). Not available lessons stay neutral, never red. */
@Composable
internal fun LessonLinkBlock(block: LessonLink) {
    val env = LocalSdui.current
    val target = env.host.lessonLinkTarget(block.lessonId)
    val colors = TeachTheme.colors
    val available = target != null
    val subtitle =
        when {
            !available -> stringResource(R.string.sdui_lesson_link_unavailable)
            target?.topicTitle != null -> stringResource(R.string.sdui_lesson_link_topic, target.topicTitle)
            else -> stringResource(R.string.sdui_lesson_link_same_topic)
        }
    val description =
        if (available) {
            stringResource(R.string.sdui_lesson_link_a11y, block.label, subtitle)
        } else {
            stringResource(R.string.sdui_lesson_link_a11y_unavailable, block.label)
        }
    TapCard(
        onClick = if (available) ({ env.host.openLesson(block.lessonId) }) else null,
        description = description,
        onClickLabel = null,
        stateText = if (available) null else stringResource(R.string.sdui_lesson_link_state_unavailable),
    ) {
        NavigationRow(
            icon = TeachIcons.ArrowRightCircle,
            iconTint = if (available) colors.inkSoft else colors.disabledInk,
            title = block.label,
            titleColor = if (available) colors.ink else colors.disabledInk,
            subtitle = subtitle,
            showChevron = available,
        )
    }
}

/** `ask_agent` (docs/design/components/ask_agent.md): an exit towards the Tutor with a ready question. */
@Composable
internal fun AskAgentBlock(block: AskAgent) {
    val env = LocalSdui.current
    val colors = TeachTheme.colors
    val title = block.label?.takeIf { it.isNotBlank() } ?: stringResource(R.string.sdui_ask_agent_default)
    val subtitle = stringResource(R.string.sdui_ask_agent_subtitle)
    TapCard(
        onClick = { env.host.askAgent(block.promptHint) },
        description = stringResource(R.string.sdui_ask_agent_a11y, title),
        onClickLabel = null,
    ) {
        NavigationRow(
            icon = TeachIcons.MessageCircle,
            iconTint = colors.accent,
            title = title,
            titleColor = colors.ink,
            subtitle = subtitle,
            showChevron = true,
        )
    }
}

@Composable
private fun NavigationRow(
    icon: Int,
    iconTint: androidx.compose.ui.graphics.Color,
    title: String,
    titleColor: androidx.compose.ui.graphics.Color,
    subtitle: String,
    showChevron: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        TeachIcon(icon, contentDescription = null, tint = iconTint)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
            TeachText(title, style = TeachTheme.typography.ui.title3, color = titleColor)
            TeachText(subtitle, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        }
        if (showChevron) TeachIcon(TeachIcons.ChevronRight, contentDescription = null, tint = TeachTheme.colors.inkSoft)
    }
}
