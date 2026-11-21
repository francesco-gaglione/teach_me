package com.teachme.core.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.StatusVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachStatusPanel
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant

/**
 * Opens Settings on the key section (`settings?section=llm-key&reason=...`). Provided once by the host of the navigation, so
 * the seven screens that can show a [LlmKeyNotice] need no extra callback.
 */
val LocalOpenLlmKeySettings = staticCompositionLocalOf<(LlmKeyVariant) -> Unit> { {} }

/** Where a key problem shows up, which fixes the words of the notice (llm-key.md sections 4, 5 and 6). */
enum class LlmKeyContext {
    /** Sent a chat message (`409`/`502` on `POST .../messages`). */
    ChatMessage,

    /** Confirmed the mission, which starts the research. */
    MissionConfirm,

    /** Asked for a new or updated research and sources are already there. */
    ResearchRequest,

    /** Same, with no sources yet. */
    ResearchRequestEmpty,
    GenerateLesson,
    CompleteLesson,

    /** The Tutor's message failed with a key code. */
    ChatReply,

    /** Same, with the part that arrived before the interruption still on screen. */
    ChatReplyPartial,
    ResearchJob,
    LessonGenerationJob,
    LessonCompletionJob,

    /** Proactive suggestions: nothing was tried (`info`, not `warning`). */
    HomeHint,
    ChatHint,
    CompleteLessonHint,
    ;

    internal val isHint: Boolean get() = this == HomeHint || this == ChatHint || this == CompleteLessonHint

    /** The failure of a job or a message that already happened, as opposed to a request refused at once. */
    internal val isFailure: Boolean
        get() =
            this == ChatReply ||
                this == ChatReplyPartial ||
                this == ResearchJob ||
                this == LessonGenerationJob ||
                this == LessonCompletionJob
}

/** How the notice looks (llm-key.md section 2, "Fasi"). */
enum class LlmKeyPhase {
    /** The backend refused (or the job failed) because of the key. */
    Blocking,

    /** The user saved a key meanwhile: for a failed job or message the notice offers "Riprova". */
    Fixed,
}

/** What the app knows about the key, provided once by the host: the notices and hints read it, no screen passes it down. */
val LocalLlmKeyStatus = staticCompositionLocalOf { LlmKeyStatus() }

/** `Fixed` once the key changed after the notice was born, `Blocking` otherwise. */
fun llmKeyPhase(
    bornVersion: Int,
    currentVersion: Int,
): LlmKeyPhase = if (currentVersion > bornVersion) LlmKeyPhase.Fixed else LlmKeyPhase.Blocking

/**
 * Remembers the key version the notice was born with. [identity] names the failure (job id, message id, a counter of
 * refusals): a new failure is a new notice, a screen that comes back after Settings is the same one. Survives the
 * screen leaving the composition while Settings is on top.
 */
@Composable
fun rememberLlmKeyPhase(
    identity: Any,
    currentVersion: Int,
): LlmKeyPhase {
    val born = rememberSaveable(identity.toString()) { currentVersion }
    return llmKeyPhase(born, currentVersion)
}

/**
 * `StatusPanel` plus one button, shared by every place that meets a missing or refused key (base-components section 16).
 * The panel is not dismissible. The phase follows the key version the host provides ([LocalLlmKeyStatus]): a request
 * refused at once ([context] not a failure) disappears when the key changes, because the user simply asks again; a failed
 * job or message turns into "Chiave salvata" with [onRetry]. [identity] names this failure (see [rememberLlmKeyPhase]).
 *
 * [LlmKeyVariant.Credit] is different (llm-key.md section 9): one button, "Riprova" ([onRetry], absent for a message that
 * cannot be retried), no way to Settings, no phase. [retrying] shows "Riprovo…"; [again] (above 0, a new value for each new
 * occurrence) announces once that the credit is still exhausted. [retryEnabled] false disables "Riprova" when there is
 * nothing to send again (a chat composer the user emptied).
 */
@Composable
fun LlmKeyNotice(
    variant: LlmKeyVariant,
    context: LlmKeyContext,
    identity: Any,
    modifier: Modifier = Modifier,
    onRetry: (() -> Unit)? = null,
    onOpenSettings: (() -> Unit)? = null,
    retrying: Boolean = false,
    again: Int = 0,
    retryEnabled: Boolean = true,
) {
    val version = LocalLlmKeyStatus.current.version
    // The credit has no `Fixed` phase (llm-key.md 9.1): nothing tells the app that the user topped it up.
    val phase =
        if (context.isHint || variant == LlmKeyVariant.Credit) {
            LlmKeyPhase.Blocking
        } else {
            rememberLlmKeyPhase(identity, version)
        }
    // The panel only shows: a key problem is reported to the monitor where the answer arrives (view models), never from
    // a panel that is shown again. An old failed job coming back into composition must not re-arm a refusal that the user
    // fixed or a success cleared.
    LlmKeyNoticePanel(variant, context, phase, modifier, onRetry, onOpenSettings, retrying, again, retryEnabled)
}

/** The suggestion for a key known to be missing or refused, nothing when the key is fine (llm-key.md section 4). */
@Composable
fun LlmKeyHint(
    context: LlmKeyContext,
    modifier: Modifier = Modifier,
) {
    val variant = LocalLlmKeyStatus.current.hintVariant ?: return
    LlmKeyNotice(variant, context, identity = "hint", modifier = modifier)
}

/** The panel for a given [phase]; [LlmKeyNotice] decides the phase, previews can force one. */
@Composable
fun LlmKeyNoticePanel(
    variant: LlmKeyVariant,
    context: LlmKeyContext,
    phase: LlmKeyPhase,
    modifier: Modifier = Modifier,
    onRetry: (() -> Unit)? = null,
    onOpenSettings: (() -> Unit)? = null,
    retrying: Boolean = false,
    again: Int = 0,
    retryEnabled: Boolean = true,
) {
    if (variant == LlmKeyVariant.Credit) {
        CreditPanel(context, modifier, onRetry, retrying, again, retryEnabled)
        return
    }
    val openKeySettings = LocalOpenLlmKeySettings.current
    val open = onOpenSettings ?: { openKeySettings(variant) }
    if (phase == LlmKeyPhase.Fixed) {
        // A refused request just goes away: the user asks again. A failed job or message offers "Riprova".
        if (!context.isFailure) return
        TeachStatusPanel(
            StatusVariant.Info,
            stringResource(R.string.llmkey_fixed_title),
            modifier.fillMaxWidth(),
            body = stringResource(R.string.llmkey_fixed_body),
        ) {
            if (onRetry != null) {
                TeachButton(
                    stringResource(R.string.llmkey_retry),
                    onRetry,
                    Modifier.padding(top = TeachTheme.spacing.xs),
                    variant = ButtonVariant.Secondary,
                )
            }
        }
        return
    }
    val missing = variant == LlmKeyVariant.Missing
    val text = noticeTexts(variant, context)
    TeachStatusPanel(
        if (context.isHint) StatusVariant.Info else StatusVariant.Warning,
        text.title,
        modifier.fillMaxWidth(),
        body = text.body,
        icon = TeachIcons.KeyRound,
    ) {
        TeachButton(
            stringResource(if (missing) R.string.llmkey_button_missing else R.string.llmkey_button_rejected),
            open,
            Modifier.padding(top = TeachTheme.spacing.xs),
            variant = ButtonVariant.Secondary,
            leadingIcon = TeachIcons.KeyRound,
            contentDescription =
                stringResource(
                    if (missing) R.string.llmkey_button_missing_desc else R.string.llmkey_button_rejected_desc,
                ),
        )
    }
}

/** The panel of an exhausted credit: a warning with the wallet, and "Riprova" as the only action (llm-key.md 9.1). */
@Composable
private fun CreditPanel(
    context: LlmKeyContext,
    modifier: Modifier,
    onRetry: (() -> Unit)?,
    retrying: Boolean,
    again: Int,
    retryEnabled: Boolean,
) {
    val text = noticeTexts(LlmKeyVariant.Credit, context)
    TeachStatusPanel(
        StatusVariant.Warning,
        text.title,
        modifier.fillMaxWidth(),
        body = text.body,
        icon = TeachIcons.Wallet,
    ) {
        if (onRetry != null) {
            TeachButton(
                stringResource(if (retrying) R.string.llm_credit_retrying else R.string.llm_credit_retry),
                onRetry,
                Modifier.padding(top = TeachTheme.spacing.xs),
                variant = ButtonVariant.Secondary,
                enabled = retryEnabled,
                loading = retrying,
                leadingIcon = TeachIcons.RotateCcw,
                contentDescription = stringResource(R.string.llm_credit_retry_cd, text.action),
            )
        }
    }
    if (again > 0) {
        // Invisible live region: the new panel replaces the old one, so say once that nothing changed.
        val announcement = stringResource(R.string.llm_credit_announce_again)
        key(again) {
            Box(
                Modifier
                    .size(1.dp)
                    .semantics {
                        liveRegion = LiveRegionMode.Polite
                        contentDescription = announcement
                    },
            )
        }
    }
}

/** [action] is the "{azione}" of the context, for the accessible name of "Riprova". */
internal data class NoticeTexts(
    val title: String,
    val body: String,
    val action: String = "",
)

@Composable
internal fun noticeTexts(
    variant: LlmKeyVariant,
    context: LlmKeyContext,
): NoticeTexts {
    val missing = variant == LlmKeyVariant.Missing
    val title =
        stringResource(
            when (variant) {
                LlmKeyVariant.Missing -> R.string.llmkey_title_missing
                LlmKeyVariant.Rejected -> R.string.llmkey_title_rejected
                LlmKeyVariant.Credit -> R.string.llm_credit_title
            },
        )
    return when (context) {
        LlmKeyContext.HomeHint ->
            NoticeTexts(
                stringResource(
                    if (missing) R.string.llmkey_hint_home_missing_title else R.string.llmkey_title_rejected,
                ),
                stringResource(
                    if (missing) R.string.llmkey_hint_home_missing_body else R.string.llmkey_hint_home_rejected_body,
                ),
            )
        LlmKeyContext.ChatHint ->
            NoticeTexts(
                stringResource(
                    if (missing) R.string.llmkey_hint_chat_missing_title else R.string.llmkey_title_rejected,
                ),
                stringResource(
                    if (missing) R.string.llmkey_hint_chat_missing_body else R.string.llmkey_hint_chat_rejected_body,
                ),
            )
        LlmKeyContext.CompleteLessonHint ->
            NoticeTexts(
                stringResource(
                    if (missing) R.string.llmkey_hint_complete_missing_title else R.string.llmkey_title_rejected,
                ),
                stringResource(
                    if (missing) {
                        R.string.llmkey_hint_complete_missing_body
                    } else {
                        R.string.llmkey_hint_complete_rejected_body
                    },
                ),
            )
        LlmKeyContext.ChatMessage ->
            request(
                variant,
                title,
                R.string.llmkey_ctx_chat_action,
                R.string.llmkey_ctx_chat_reassure,
            )
        LlmKeyContext.MissionConfirm ->
            request(variant, title, R.string.llmkey_ctx_mission_action, R.string.llmkey_ctx_mission_reassure)
        LlmKeyContext.ResearchRequest ->
            request(variant, title, R.string.llmkey_ctx_research_action, R.string.llmkey_ctx_research_reassure)
        LlmKeyContext.ResearchRequestEmpty ->
            request(variant, title, R.string.llmkey_ctx_research_action, R.string.llmkey_ctx_research_reassure_empty)
        LlmKeyContext.GenerateLesson ->
            request(variant, title, R.string.llmkey_ctx_generate_action, R.string.llmkey_ctx_generate_reassure)
        LlmKeyContext.CompleteLesson ->
            request(variant, title, R.string.llmkey_ctx_complete_action, R.string.llmkey_ctx_complete_reassure)
        LlmKeyContext.ChatReply, LlmKeyContext.ChatReplyPartial ->
            failure(
                variant,
                title,
                if (missing) R.string.llmkey_fail_reply_action_missing else R.string.llmkey_fail_reply_action_rejected,
                if (context == LlmKeyContext.ChatReply) {
                    R.string.llmkey_fail_reply_reassure
                } else {
                    R.string.llmkey_fail_reply_reassure_partial
                },
            )
        LlmKeyContext.ResearchJob ->
            failure(variant, title, R.string.llmkey_fail_research_action, R.string.llmkey_fail_research_reassure)
        LlmKeyContext.LessonGenerationJob ->
            failure(variant, title, R.string.llmkey_fail_generate_action, R.string.llmkey_fail_generate_reassure)
        LlmKeyContext.LessonCompletionJob ->
            failure(variant, title, R.string.llmkey_fail_complete_action, R.string.llmkey_fail_complete_reassure)
    }
}

@Composable
private fun request(
    variant: LlmKeyVariant,
    title: String,
    action: Int,
    reassure: Int,
) = body(
    variant,
    title,
    stringResource(action),
    stringResource(reassure),
    R.string.llmkey_request_missing,
    R.string.llmkey_request_rejected,
    R.string.llm_credit_body_request,
)

@Composable
private fun failure(
    variant: LlmKeyVariant,
    title: String,
    action: Int,
    reassure: Int,
) = body(
    variant,
    title,
    stringResource(action),
    stringResource(reassure),
    R.string.llmkey_failure_missing,
    R.string.llmkey_failure_rejected,
    R.string.llm_credit_body_failure,
)

@Composable
private fun body(
    variant: LlmKeyVariant,
    title: String,
    action: String,
    reassure: String,
    missing: Int,
    rejected: Int,
    credit: Int,
) = NoticeTexts(
    title,
    stringResource(
        when (variant) {
            LlmKeyVariant.Missing -> missing
            LlmKeyVariant.Rejected -> rejected
            LlmKeyVariant.Credit -> credit
        },
        action,
        reassure,
    ),
    action,
)

@Composable
private fun NoticeSamples() {
    Column(
        Modifier.padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.md),
    ) {
        LlmKeyNoticePanel(LlmKeyVariant.Missing, LlmKeyContext.ChatMessage, LlmKeyPhase.Blocking)
        LlmKeyNoticePanel(LlmKeyVariant.Rejected, LlmKeyContext.LessonGenerationJob, LlmKeyPhase.Blocking)
        LlmKeyNoticePanel(LlmKeyVariant.Rejected, LlmKeyContext.LessonGenerationJob, LlmKeyPhase.Fixed, onRetry = {})
        LlmKeyNoticePanel(LlmKeyVariant.Missing, LlmKeyContext.HomeHint, LlmKeyPhase.Blocking)
        // Credit: the request refused at once, the job that failed, and the retry in flight.
        LlmKeyNoticePanel(LlmKeyVariant.Credit, LlmKeyContext.ChatMessage, LlmKeyPhase.Blocking, onRetry = {})
        LlmKeyNoticePanel(LlmKeyVariant.Credit, LlmKeyContext.LessonGenerationJob, LlmKeyPhase.Blocking, onRetry = {})
        LlmKeyNoticePanel(
            LlmKeyVariant.Credit,
            LlmKeyContext.LessonGenerationJob,
            LlmKeyPhase.Blocking,
            onRetry = {},
            retrying = true,
        )
        // An older message of the chat cannot be retried: body only.
        LlmKeyNoticePanel(LlmKeyVariant.Credit, LlmKeyContext.ChatReplyPartial, LlmKeyPhase.Blocking)
    }
}

@Preview(name = "Key notices light", showBackground = true)
@Composable
private fun NoticesLightPreview() = TeachTheme(darkTheme = false) { NoticeSamples() }

@Preview(name = "Key notices dark", showBackground = true)
@Composable
private fun NoticesDarkPreview() = TeachTheme(darkTheme = true) { NoticeSamples() }

@Preview(name = "Key notices font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun NoticesLargeFontPreview() = TeachTheme(darkTheme = false) { NoticeSamples() }
