package com.teachme.feature.lesson

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.ButtonSize
import com.teachme.core.designsystem.components.ButtonVariant
import com.teachme.core.designsystem.components.TeachButton
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.ui.llmBudgetAvailability
import com.teachme.core.ui.rememberBudgetRenewed
import java.time.Instant

/**
 * "Continua" / "Completa lezione" (docs/design/screens/lesson.md section 5). While a required exercise is
 * missing it is a soft lock: a secondary button with a lock and the reason above it, never a grey dead
 * button. Tapping it takes the learner to the missing exercise ([onLockedTap]).
 */
@Composable
fun ContinueBar(
    isLast: Boolean,
    missing: Int,
    onContinue: () -> Unit,
    onLockedTap: () -> Unit,
    modifier: Modifier = Modifier,
    /** The answers are being sent before the lesson closes (lesson.md section 7.2). */
    busy: Boolean = false,
    /** "Completa lezione" was refused for the daily LLM budget: it waits for this instant (llm-key.md 10.1). */
    budgetResetAt: Instant? = null,
) {
    val budgetWait = budgetResetAt != null && !rememberBudgetRenewed(budgetResetAt, announce = false)
    val budgetAvailability = budgetResetAt?.let { llmBudgetAvailability(it) }
    val label =
        stringResource(
            when {
                busy -> R.string.lesson_sending_answers
                isLast -> R.string.lesson_complete
                else -> R.string.lesson_continue
            },
        )
    var pointed by remember(missing) { mutableStateOf(false) }
    val view = LocalView.current
    val unlockedAnnouncement = stringResource(R.string.lesson_unlocked_announce)
    var wasLocked by remember { mutableStateOf(missing > 0) }
    LaunchedEffect(missing) {
        if (missing == 0 && wasLocked) {
            @Suppress("DEPRECATION")
            view.announceForAccessibility(unlockedAnnouncement)
        }
        wasLocked = missing > 0
    }
    if (missing == 0) {
        TeachButton(
            label,
            onContinue,
            modifier.fillMaxWidth(),
            size = ButtonSize.Large,
            enabled = !budgetWait,
            loading = busy,
            disabledStateDescription = budgetAvailability,
        )
        return
    }
    val reason =
        when {
            pointed -> stringResource(R.string.lesson_locked_pointed)
            missing == 1 -> stringResource(R.string.lesson_locked_one)
            else -> pluralStringResource(R.plurals.lesson_missing_count, missing, missing)
        }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        Row(
            Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs),
        ) {
            TeachIcon(
                TeachIcons.Lock,
                contentDescription = null,
                tint = TeachTheme.colors.inkSoft,
                size = TeachTheme.layout.iconSizeSmall,
            )
            TeachText(reason, style = TeachTheme.typography.ui.body, color = TeachTheme.colors.inkSoft)
        }
        val state = stringResource(R.string.lesson_locked_state, reason)
        TeachButton(
            text = label,
            onClick = {
                pointed = true
                onLockedTap()
            },
            modifier = Modifier.fillMaxWidth().semantics { stateDescription = state },
            variant = ButtonVariant.Secondary,
            size = ButtonSize.Large,
            leadingIcon = TeachIcons.Lock,
        )
    }
}

@Composable
private fun ContinueSample() {
    Column(
        Modifier.background(TeachTheme.colors.bg).padding(TeachTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.lg),
    ) {
        ContinueBar(isLast = false, missing = 1, onContinue = {}, onLockedTap = {})
        ContinueBar(isLast = false, missing = 2, onContinue = {}, onLockedTap = {})
        ContinueBar(isLast = false, missing = 0, onContinue = {}, onLockedTap = {})
        ContinueBar(isLast = true, missing = 0, onContinue = {}, onLockedTap = {})
    }
}

@Preview(name = "Continue light", showBackground = true)
@Composable
private fun ContinueLightPreview() = TeachTheme(darkTheme = false) { ContinueSample() }

@Preview(name = "Continue dark", showBackground = true)
@Composable
private fun ContinueDarkPreview() = TeachTheme(darkTheme = true) { ContinueSample() }
