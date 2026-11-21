package com.teachme.feature.mission

import com.teachme.core.model.KeyNotice
import com.teachme.core.model.Mission
import com.teachme.core.model.TopicMission
import com.teachme.core.model.asStartingPoint
import com.teachme.core.model.dayMonthFormatter
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.util.Locale

/** Where a confirmation of the mission stands (one at a time per Topic). */
sealed interface ConfirmState {
    data object Idle : ConfirmState

    data class Confirming(
        val version: Int,
    ) : ConfirmState

    /**
     * Network or server failure: the card goes back to "open" with an error and a "Riprova". With a [keyNotice] the
     * backend refused only because of the LLM key (llm-key.md section 5): nothing was confirmed, the proposal stays open
     * and the error slot holds the key notice instead.
     */
    data class Failed(
        val version: Int,
        val keyNotice: KeyNotice? = null,
    ) : ConfirmState
}

/** States of a proposal card (docs/design/screens/mission.md section 2.2). */
enum class MissionCardState {
    Open,
    Arriving,
    Confirming,
    ConfirmError,
    Confirmed,
    Superseded,

    /** The mission data is not loaded (yet): the content is shown, without actions. */
    Unknown,
}

/**
 * Decides the state of the card of [proposal] from the data (mission.md contract section 3): never from a
 * flag in the message and never from the Topic chip, because a change proposal does not change the Topic state.
 * [turnBusy] is true while the Tutor is writing (this includes a message that is still streaming).
 *
 * [replacesVersion] is the `replaces_version` of a proposal part. When it names a version that is not the current one
 * (another version was confirmed since), the proposal was written against a mission that is gone: the contract does not
 * say what to do, so the card takes the most prudent reading, "superseded, not applicable", and never offers to confirm.
 * Null skips the check (the Mission tab shows the open draft against the current version by construction).
 */
fun missionCardState(
    proposal: Mission,
    mission: TopicMission?,
    turnBusy: Boolean,
    confirm: ConfirmState,
    replacesVersion: Int? = null,
): MissionCardState {
    if (mission == null) return MissionCardState.Unknown
    if (proposal.version == mission.current?.version) return MissionCardState.Confirmed
    // A version newer than anything we know: our data is behind, do not call it superseded.
    if (proposal.version > maxOf(mission.current?.version ?: 0, mission.draft?.version ?: 0)) {
        return MissionCardState.Unknown
    }
    if (proposal.version != mission.draft?.version) return MissionCardState.Superseded
    if (replacesVersion != null && replacesVersion != mission.current?.version) return MissionCardState.Superseded
    return when {
        confirm is ConfirmState.Confirming && confirm.version == proposal.version -> MissionCardState.Confirming
        confirm is ConfirmState.Failed && confirm.version == proposal.version -> MissionCardState.ConfirmError
        turnBusy -> MissionCardState.Arriving
        else -> MissionCardState.Open
    }
}

/**
 * How the starting point of a draft differs from the confirmed version it replaces (mission.md design section 2.5).
 * A blank value counts as none; the comparison is an exact string comparison.
 */
sealed interface StartingPointChange {
    /** Same text, or none on both sides. */
    data object Unchanged : StartingPointChange

    /** The confirmed version had none: "Nuovo" pill, no "before" text. */
    data object Added : StartingPointChange

    /** Different text: "Nuovo" pill and a closed "Com'era nella versione N" with [previous]. */
    data class Changed(
        val previous: String,
    ) : StartingPointChange

    /** The draft has none but the confirmed version had one (the tool forbids it): a defensive line. */
    data object Removed : StartingPointChange
}

fun startingPointChange(
    draft: String?,
    previous: String?,
): StartingPointChange {
    val now = draft.asStartingPoint()
    val before = previous.asStartingPoint()
    return when {
        now == before -> StartingPointChange.Unchanged
        before == null -> StartingPointChange.Added
        now == null -> StartingPointChange.Removed
        else -> StartingPointChange.Changed(before)
    }
}

/**
 * The invitation to say what one already knows (mission.md design section 4.6): only on the current mission,
 * only while it has no starting point and no pending draft brings one. Never for an absent mission.
 */
fun showStartingPointInvite(mission: TopicMission?): Boolean {
    val current = mission?.current ?: return false
    return current.startingPoint.asStartingPoint() == null && mission.draft?.startingPoint.asStartingPoint() == null
}

/**
 * What changed in a draft compared with the confirmed version (exact string comparison, mission.md design
 * section 2.1). Indexes refer to the draft's lists; [removed] are the confirmed entries that are gone.
 */
data class MissionDiff(
    val whyChanged: Boolean,
    val startingPoint: StartingPointChange,
    val newCriteria: Set<Int>,
    val newConstraints: Set<Int>,
    val newOutOfScope: Set<Int>,
    val removed: List<String>,
    val previousVersion: Int,
) {
    val isEmpty: Boolean
        get() =
            !whyChanged &&
                startingPoint == StartingPointChange.Unchanged &&
                newCriteria.isEmpty() &&
                newConstraints.isEmpty() &&
                newOutOfScope.isEmpty() &&
                removed.isEmpty()
}

fun missionDiff(
    draft: Mission,
    previous: Mission,
): MissionDiff {
    fun fresh(
        now: List<String>,
        before: List<String>,
    ) = now.indices.filter { now[it] !in before }.toSet()

    fun gone(
        now: List<String>,
        before: List<String>,
    ) = before.filter { it !in now }
    return MissionDiff(
        whyChanged = draft.why != previous.why,
        startingPoint = startingPointChange(draft.startingPoint, previous.startingPoint),
        newCriteria = fresh(draft.successCriteria, previous.successCriteria),
        newConstraints = fresh(draft.constraints, previous.constraints),
        newOutOfScope = fresh(draft.outOfScope, previous.outOfScope),
        removed =
            gone(draft.successCriteria, previous.successCriteria) +
                gone(draft.constraints, previous.constraints) +
                gone(draft.outOfScope, previous.outOfScope),
        previousVersion = previous.version,
    )
}

/** The diff to show on a card that proposes [draft], when the confirmed version it replaces is known. */
fun diffAgainstCurrent(
    draft: Mission,
    replacesVersion: Int?,
    mission: TopicMission?,
): MissionDiff? {
    val current = mission?.current ?: return null
    if (replacesVersion == null || current.version != replacesVersion) return null
    return missionDiff(draft, current).takeUnless { it.isEmpty }
}

/** "8 novembre" / "November 8" by [locale], with the year only when it is not the current one (never a numeric format). */
fun formatMissionDate(
    instant: Instant,
    today: LocalDate,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): String {
    val date = instant.atZone(zone).toLocalDate()
    return date.format(dayMonthFormatter(locale, withYear = date.year != today.year))
}

/**
 * How many versions exist besides the current one and the open draft. Versions are numbered 1..N without
 * gaps (contract section 2), so this is known before the list is fetched.
 */
fun otherVersionsCount(mission: TopicMission?): Int {
    val top = maxOf(mission?.current?.version ?: 0, mission?.draft?.version ?: 0)
    val shown = listOfNotNull(mission?.current, mission?.draft).size
    return (top - shown).coerceAtLeast(0)
}

/** First [max] characters, with an ellipsis when cut (used in accessibility names of preferences). */
fun String.ellipsize(max: Int): String = if (length <= max) this else take(max).trimEnd() + "…"

/** Max characters of a preference quoted in a content description (mission.md design section 4.4). */
const val PREFERENCE_QUOTE_MAX = 40

/** Versions listed before "Mostra altre" (base-components section 13). */
const val VERSIONS_PAGE_SIZE = 10
