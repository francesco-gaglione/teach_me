package com.teachme.feature.mission

import com.teachme.core.model.Mission
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.TopicMission
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import java.util.Locale

class MissionLogicTest {
    private fun mission(
        version: Int,
        status: MissionStatus = MissionStatus.Draft,
        why: String = "Perché",
        criteria: List<String> = listOf("a"),
        constraints: List<String> = emptyList(),
        out: List<String> = emptyList(),
        start: String? = null,
    ) = Mission(
        "t1",
        version,
        status,
        why,
        criteria,
        constraints,
        out,
        Instant.EPOCH,
        Instant.EPOCH.takeIf { status == MissionStatus.Confirmed },
        null,
        start,
    )

    private val confirmed1 = mission(1, MissionStatus.Confirmed)
    private val draft2 = mission(2)

    private fun state(
        proposal: Mission,
        data: TopicMission?,
        busy: Boolean = false,
        confirm: ConfirmState = ConfirmState.Idle,
    ) = missionCardState(proposal, data, busy, confirm)

    @Test
    fun `without data the card has no actions`() {
        assertEquals(MissionCardState.Unknown, state(draft2, null))
    }

    @Test
    fun `the open draft is open, arriving while the tutor writes`() {
        val data = TopicMission(null, draft2)
        assertEquals(MissionCardState.Open, state(draft2, data))
        assertEquals(MissionCardState.Arriving, state(draft2, data, busy = true))
    }

    @Test
    fun `a change proposal is open even though the topic keeps its state`() {
        // The state comes from the data (draft present), never from a Topic state.
        assertEquals(MissionCardState.Open, state(draft2, TopicMission(confirmed1, draft2)))
    }

    @Test
    fun `confirming and error follow the confirm state of that version only`() {
        val data = TopicMission(null, draft2)
        assertEquals(MissionCardState.Confirming, state(draft2, data, true, ConfirmState.Confirming(2)))
        assertEquals(MissionCardState.ConfirmError, state(draft2, data, false, ConfirmState.Failed(2)))
        assertEquals(MissionCardState.Open, state(draft2, data, false, ConfirmState.Failed(7)))
    }

    @Test
    fun `the version that became current is confirmed, even while a newer draft waits`() {
        assertEquals(MissionCardState.Confirmed, state(draft2, TopicMission(mission(2, MissionStatus.Confirmed), null)))
        assertEquals(
            MissionCardState.Confirmed,
            state(draft2, TopicMission(mission(2, MissionStatus.Confirmed), mission(3))),
        )
    }

    @Test
    fun `any other version is superseded`() {
        assertEquals(MissionCardState.Superseded, state(draft2, TopicMission(null, mission(3))))
        assertEquals(
            MissionCardState.Superseded,
            state(draft2, TopicMission(mission(3, MissionStatus.Confirmed), null)),
        )
        // Newer than anything known: our data is behind, so no verdict.
        assertEquals(MissionCardState.Unknown, state(draft2, TopicMission(null, null)))
        assertEquals(MissionCardState.Unknown, state(mission(5), TopicMission(confirmed1, draft2)))
    }

    @Test
    fun `diff marks new and changed entries and lists the removed ones`() {
        val before =
            mission(1, MissionStatus.Confirmed, "Lavoro", listOf("a", "b", "c"), listOf("30 min"), listOf("embedded"))
        val after =
            mission(
                2,
                why = "Lavoro e altro",
                criteria = listOf("a", "b2"),
                constraints = listOf("30 min", "Kotlin"),
                out = emptyList(),
            )
        val diff = missionDiff(after, before)
        assertTrue(diff.whyChanged)
        assertEquals(setOf(1), diff.newCriteria)
        assertEquals(setOf(1), diff.newConstraints)
        assertEquals(emptySet<Int>(), diff.newOutOfScope)
        assertEquals(listOf("b", "c", "embedded"), diff.removed)
        assertEquals(1, diff.previousVersion)
    }

    @Test
    fun `identical text gives an empty diff and no marks`() {
        val diff = missionDiff(draft2, confirmed1)
        assertTrue(diff.isEmpty)
        assertNull(diffAgainstCurrent(draft2, 1, TopicMission(confirmed1, draft2)))
    }

    @Test
    fun `the diff is computed only against the version the proposal replaces`() {
        val changed = mission(2, criteria = listOf("z"))
        assertTrue(diffAgainstCurrent(changed, 1, TopicMission(confirmed1, changed)) != null)
        assertNull(diffAgainstCurrent(changed, 5, TopicMission(confirmed1, changed)))
        assertNull(diffAgainstCurrent(changed, null, TopicMission(confirmed1, changed)))
        assertNull(diffAgainstCurrent(changed, 1, TopicMission(null, changed)))
    }

    @Test
    fun `a proposal that replaces another version than the current one is not applicable`() {
        val data = TopicMission(confirmed1, draft2)
        assertEquals(MissionCardState.Open, missionCardState(draft2, data, false, ConfirmState.Idle, 1))
        // Written against version 5, but the current one is 1: no buttons, whatever the rest says.
        assertEquals(MissionCardState.Superseded, missionCardState(draft2, data, false, ConfirmState.Idle, 5))
        assertEquals(
            MissionCardState.Superseded,
            missionCardState(draft2, data, false, ConfirmState.Failed(2), 5),
        )
        // No `replaces_version` (first proposal) or no check: unchanged.
        assertEquals(
            MissionCardState.Open,
            missionCardState(draft2, TopicMission(null, draft2), false, ConfirmState.Idle),
        )
    }

    @Test
    fun `dates are day and month, with the year only when it is not this year`() {
        val today = LocalDate.of(2026, 11, 8)
        assertEquals(
            "8 novembre",
            formatMissionDate(Instant.parse("2026-11-08T10:00:00Z"), today, ZoneOffset.UTC, Locale.ITALIAN),
        )
        assertEquals(
            "November 8",
            formatMissionDate(Instant.parse("2026-11-08T10:00:00Z"), today, ZoneOffset.UTC, Locale.US),
        )
        assertEquals(
            "3 marzo 2025",
            formatMissionDate(Instant.parse("2025-03-03T10:00:00Z"), today, ZoneOffset.UTC, Locale.ITALIAN),
        )
    }

    @Test
    fun `other versions are counted from the numbering`() {
        assertEquals(0, otherVersionsCount(null))
        assertEquals(0, otherVersionsCount(TopicMission(null, mission(1))))
        assertEquals(1, otherVersionsCount(TopicMission(mission(2, MissionStatus.Confirmed), null)))
        assertEquals(1, otherVersionsCount(TopicMission(mission(2, MissionStatus.Confirmed), mission(3))))
        assertEquals(2, otherVersionsCount(TopicMission(null, mission(3))))
    }

    @Test
    fun `quotes are cut with an ellipsis`() {
        assertEquals("abc", "abc".ellipsize(40))
        assertEquals("abcd…", "abcdefgh".ellipsize(4))
        assertFalse("x".ellipsize(1).endsWith("…"))
    }

    // Starting point (mission.md design section 2.5): the four rows of the comparison table.

    private val knowsKotlin = "Programmo in Kotlin da tre anni."

    @Test
    fun `starting point added to a mission that had none`() {
        val before = mission(1, MissionStatus.Confirmed)
        val diff = missionDiff(mission(2, start = knowsKotlin), before)
        assertEquals(StartingPointChange.Added, diff.startingPoint)
        assertFalse(diff.isEmpty)
        assertTrue(diffAgainstCurrent(mission(2, start = knowsKotlin), 1, TopicMission(before, null)) != null)
    }

    @Test
    fun `starting point changed keeps the previous text`() {
        val before = mission(1, MissionStatus.Confirmed, start = knowsKotlin)
        val diff = missionDiff(mission(2, start = "Ho finito il libro."), before)
        assertEquals(StartingPointChange.Changed(knowsKotlin), diff.startingPoint)
        assertFalse(diff.isEmpty)
    }

    @Test
    fun `same starting point is no change`() {
        val before = mission(1, MissionStatus.Confirmed, start = knowsKotlin)
        val diff = missionDiff(mission(2, start = knowsKotlin), before)
        assertEquals(StartingPointChange.Unchanged, diff.startingPoint)
        assertTrue(diff.isEmpty)
    }

    @Test
    fun `starting point dropped is the defensive case`() {
        val before = mission(1, MissionStatus.Confirmed, start = knowsKotlin)
        val diff = missionDiff(mission(2), before)
        assertEquals(StartingPointChange.Removed, diff.startingPoint)
        assertFalse(diff.isEmpty)
    }

    @Test
    fun `a blank starting point is the same as none`() {
        assertEquals(StartingPointChange.Unchanged, startingPointChange("  \n", null))
        assertEquals(StartingPointChange.Unchanged, startingPointChange(null, ""))
        assertEquals(StartingPointChange.Added, startingPointChange("x", " "))
        assertEquals(StartingPointChange.Removed, startingPointChange("  ", "x"))
    }

    @Test
    fun `the invitation shows on the current mission without a starting point`() {
        assertTrue(showStartingPointInvite(TopicMission(mission(1, MissionStatus.Confirmed), null)))
        assertTrue(showStartingPointInvite(TopicMission(mission(1, MissionStatus.Confirmed, start = " "), null)))
        // A draft without it does not resolve the invitation either.
        assertTrue(showStartingPointInvite(TopicMission(mission(1, MissionStatus.Confirmed), mission(2))))
    }

    @Test
    fun `the invitation is gone when the mission or a pending draft has a starting point`() {
        assertFalse(
            showStartingPointInvite(
                TopicMission(mission(1, MissionStatus.Confirmed, start = knowsKotlin), null),
            ),
        )
        assertFalse(
            showStartingPointInvite(
                TopicMission(mission(1, MissionStatus.Confirmed), mission(2, start = knowsKotlin)),
            ),
        )
    }

    @Test
    fun `no invitation without a current mission`() {
        assertFalse(showStartingPointInvite(null))
        assertFalse(showStartingPointInvite(TopicMission(null, null)))
        assertFalse(showStartingPointInvite(TopicMission(null, mission(1))))
    }
}
