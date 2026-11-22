package com.teachme.feature.resources

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.WindowWidthClass
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachDisclosure
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachListCard
import com.teachme.core.designsystem.components.TeachRowDivider
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.designsystem.components.TeachSurface
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.currentWindowWidthClass
import com.teachme.core.model.Gap
import com.teachme.core.model.JobError
import com.teachme.core.model.KeyNotice
import com.teachme.core.model.LoadError
import com.teachme.core.model.Resource
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.budgetResetInstant
import com.teachme.core.model.isLlmBudgetCode
import com.teachme.core.model.llmKeyVariantOfCode
import com.teachme.core.ui.LlmBudgetPanel
import com.teachme.core.ui.LlmKeyContext
import com.teachme.core.ui.LlmKeyNotice
import java.time.Instant

private val BULLET_COLUMN = 20.dp

/** Knowledge and community groups, with the opt-out sentence in place of an empty community group (4.3). */
@Composable
internal fun SourceGroups(
    view: ResourcesView,
    newUrls: Set<String>,
    onOpen: (String) -> Unit,
    onCopy: (String) -> Unit,
) {
    val topGap =
        if (currentWindowWidthClass() ==
            WindowWidthClass.Compact
        ) {
            TeachTheme.spacing.lg
        } else {
            TeachTheme.spacing.xl
        }
    Column(verticalArrangement = Arrangement.spacedBy(topGap)) {
        val knowledge = view.knowledge
        val community = view.community
        if (knowledge.isNotEmpty()) {
            SourceGroup(
                title = stringResource(R.string.res_group_knowledge, knowledge.size),
                description =
                    pluralStringResource(
                        R.plurals.res_group_knowledge_description,
                        knowledge.size,
                        knowledge.size,
                    ),
                hint = stringResource(R.string.res_group_knowledge_hint),
                items = knowledge,
                newUrls = newUrls,
                onOpen = onOpen,
                onCopy = onCopy,
            )
        }
        if (community.isNotEmpty()) {
            SourceGroup(
                title = stringResource(R.string.res_group_community, community.size),
                description =
                    pluralStringResource(
                        R.plurals.res_group_community_description,
                        community.size,
                        community.size,
                    ),
                hint = stringResource(R.string.res_group_community_hint),
                items = community,
                newUrls = newUrls,
                onOpen = onOpen,
                onCopy = onCopy,
            )
        } else if (view.communitiesSkipped) {
            TeachText(
                stringResource(R.string.res_communities_skipped),
                style = TeachTheme.typography.ui.bodyS,
                color = TeachTheme.colors.inkMuted,
            )
        }
    }
}

@Composable
private fun SourceGroup(
    title: String,
    description: String,
    hint: String,
    items: List<Resource>,
    newUrls: Set<String>,
    onOpen: (String) -> Unit,
    onCopy: (String) -> Unit,
) {
    Column {
        TeachText(
            title,
            Modifier.semantics {
                heading()
                contentDescription = description
            },
            style = TeachTheme.typography.ui.title3,
        )
        TeachText(
            hint,
            Modifier.padding(top = TeachTheme.spacing.xs),
            style = TeachTheme.typography.ui.bodyS,
            color = TeachTheme.colors.inkSoft,
        )
        TeachListCard(Modifier.padding(top = TeachTheme.spacing.sm)) {
            Column {
                items.forEachIndexed { index, resource ->
                    if (index > 0) TeachRowDivider()
                    ResourceRow(resource, isNew = resource.url != null && resource.url in newUrls, onOpen, onCopy)
                }
            }
        }
    }
}

/** "Cosa non ho trovato" (resources.md section 5): calm, last, never a counter or a warning color. */
@Composable
internal fun GapsBlock(
    gaps: List<Gap>,
    modifier: Modifier = Modifier,
) {
    if (gaps.isEmpty()) return
    val colors = TeachTheme.colors
    val title = stringResource(R.string.res_gaps_title)
    var all by rememberSaveable { mutableStateOf(false) }
    val shown = if (all || gaps.size <= GAPS_VISIBLE) gaps else gaps.take(GAPS_VISIBLE)
    val padding = if (currentWindowWidthClass() == WindowWidthClass.Compact) TeachTheme.spacing.md else 20.dp
    TeachSurface(
        modifier.fillMaxWidth().semantics { paneTitle = title },
        color = colors.surfaceSunken,
    ) {
        Column(
            Modifier.fillMaxWidth().padding(padding),
            verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.sm),
        ) {
            Row(
                Modifier.semantics { heading() },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
            ) {
                TeachIcon(
                    TeachIcons.Compass,
                    contentDescription = null,
                    tint = colors.inkSoft,
                    size = TeachTheme.layout.iconSizeSmall,
                )
                TeachText(title, style = TeachTheme.typography.ui.label, color = colors.ink)
            }
            TeachText(
                stringResource(R.string.res_gaps_intro),
                style = TeachTheme.typography.ui.body,
                color = colors.inkSoft,
            )
            shown.forEach { gap -> GapItem(gap) }
            if (gaps.size > GAPS_VISIBLE) {
                TeachButton(
                    if (all) {
                        stringResource(R.string.res_gaps_less)
                    } else {
                        stringResource(R.string.res_gaps_more, gaps.size - GAPS_VISIBLE)
                    },
                    { all = !all },
                    variant = ButtonVariant.Text,
                )
            }
        }
    }
}

@Composable
private fun GapItem(gap: Gap) {
    val description = stringResource(R.string.res_gap_description, gap.area)
    Row(Modifier.clearAndSetSemantics { contentDescription = description + " " + gap.note }) {
        TeachText(
            "•",
            Modifier.width(BULLET_COLUMN),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xxs)) {
            TeachText(gap.area, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.ink)
            if (gap.note.isNotBlank()) {
                TeachText(gap.note, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
            }
        }
    }
}

/** The failed panel (resources.md 4.4 and 6.2): friendly title and body per error code, raw message under "Dettagli". */
@Composable
internal fun FailedPanel(
    jobId: String,
    error: JobError?,
    refreshing: Boolean,
    actionError: LoadError?,
    acting: Boolean,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
    keyNotice: KeyNotice? = null,
    /** When the job failed: the budget wait of a job that stopped for it is counted from here (it carries no Retry-After). */
    failedAt: Instant? = null,
    /** A "Riprova" refused for the daily LLM budget: when it comes back; the later reset wins (llm-key.md 10.1). */
    budgetRetryResetAt: Instant? = null,
) {
    if (error?.code.isLlmBudgetCode() || budgetRetryResetAt != null) {
        // Saved (epoch millis) so a rotation does not restart the wait.
        val fallbackMillis = rememberSaveable { System.currentTimeMillis() }
        val own =
            remember(failedAt, fallbackMillis) {
                budgetResetInstant(
                    failedAt ?: Instant.ofEpochMilli(fallbackMillis),
                )
            }
        val resetAt = if (budgetRetryResetAt != null && budgetRetryResetAt.isAfter(own)) budgetRetryResetAt else own
        LlmBudgetPanel(
            resetAt,
            modifier,
            retryText = stringResource(if (acting) R.string.res_retrying else R.string.res_retry),
            onRetry = onRetry,
            retrying = acting,
            retryIcon = TeachIcons.RotateCcw,
        )
        return
    }
    val keyVariant = error?.code.llmKeyVariantOfCode()
    if (keyVariant != null) {
        // The key, not the research, is the problem: "Riprova" only once a key has been saved (llm-key.md section 6).
        LlmKeyNotice(
            keyVariant,
            LlmKeyContext.ResearchJob,
            jobId,
            modifier,
            onRetry = onRetry,
            retrying = acting,
            again = keyNotice?.again ?: 0,
        )
        return
    }
    val kind = failureKind(error?.code)
    val title =
        when {
            refreshing -> R.string.res_failed_title_refresh
            kind == FailureKind.Upstream -> R.string.res_failed_title_upstream
            kind == FailureKind.RateLimited -> R.string.res_failed_title_rate
            kind == FailureKind.Timeout -> R.string.res_failed_title_timeout
            else -> R.string.res_failed_title_generic
        }
    val body =
        when {
            actionError == LoadError.Network || actionError == LoadError.Unauthorized -> R.string.res_failed_network
            actionError == LoadError.Server -> R.string.res_failed_server
            refreshing -> R.string.res_failed_body_refresh
            kind == FailureKind.Upstream -> R.string.res_failed_body_upstream
            kind == FailureKind.RateLimited -> R.string.res_failed_body_rate
            kind == FailureKind.Timeout -> R.string.res_failed_body_timeout
            else -> R.string.res_failed_body_generic
        }
    var details by rememberSaveable { mutableStateOf(false) }
    TeachStatusPanel(
        StatusVariant.Danger,
        stringResource(title),
        modifier,
        body = stringResource(body),
    ) {
        // The retry itself was refused for the key: the notice sits in the panel it started from (llm-key.md 5).
        keyNotice?.let { LlmKeyNotice(it.variant, LlmKeyContext.ResearchRequestEmpty, it.id) }
        TeachButton(
            if (acting) stringResource(R.string.res_retrying) else stringResource(R.string.res_retry),
            onRetry,
            variant = ButtonVariant.Secondary,
            loading = acting,
            leadingIcon = TeachIcons.RotateCcw,
        )
        val message = error?.message?.takeIf { it.isNotBlank() }
        if (message != null) {
            TeachDisclosure(stringResource(R.string.res_details), details, { details = !details }) {
                SelectionContainer {
                    TeachText(message, style = TeachTheme.typography.ui.bodyS, color = TeachTheme.colors.inkSoft)
                }
            }
        }
    }
}

/** Search finished but nothing was solid enough (C0): informative, never danger. */
@Composable
internal fun NoSourcesPanel(
    acting: Boolean,
    onSearchAgain: () -> Unit,
    modifier: Modifier = Modifier,
) {
    TeachStatusPanel(
        StatusVariant.Info,
        stringResource(R.string.res_none_title),
        modifier,
        body = stringResource(R.string.res_none_body),
        announce = false,
    ) {
        TeachButton(
            stringResource(if (acting) R.string.res_none_action_busy else R.string.res_none_action),
            onSearchAgain,
            variant = ButtonVariant.Secondary,
            loading = acting,
        )
    }
}

/** The mission changed after the search (resources.md 6.1): the only primary button of the tab. */
@Composable
internal fun StaleBanner(
    version: Int,
    acting: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
    /** A PDF was also added after the last search: one banner, one button (resources.md 13.8). */
    alsoPdf: Boolean = false,
) {
    TeachStatusPanel(
        StatusVariant.Info,
        stringResource(if (alsoPdf) R.string.res_stale_title_pdf else R.string.res_stale_title),
        modifier,
        body = stringResource(if (alsoPdf) R.string.res_stale_body_pdf else R.string.res_stale_body, version),
        announce = false,
    ) {
        TeachButton(
            stringResource(if (acting) R.string.res_refresh_busy else R.string.res_refresh),
            onRefresh,
            variant = ButtonVariant.Primary,
            loading = acting,
        )
    }
}

/** "8 fonti" for the summary. */
@Composable
internal fun countText(count: Int): String = pluralStringResource(R.plurals.res_count, count, count)
