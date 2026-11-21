package com.teachme.feature.mission

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.Mission
import com.teachme.core.model.MissionRepository
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.Preference
import com.teachme.core.model.PreferenceScope
import com.teachme.core.model.TopicMission
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

/** Remembers the credit like the real monitor: `noteObserved(Credit)` sets the flag, `noteWorked` clears it. */
private class CreditTrackingMonitor : LlmKeyMonitor by NoLlmKeyMonitor {
    private val flow = kotlinx.coroutines.flow.MutableStateFlow(LlmKeyStatus())
    override val status: kotlinx.coroutines.flow.StateFlow<LlmKeyStatus> get() = flow
    val observed = mutableListOf<LlmKeyVariant>()

    // Delegation (`by`) does not use the interface default: forward it.
    override fun noteFailedNow(
        variant: LlmKeyVariant,
        serverDate: java.time.Instant?,
    ) = noteObserved(variant, serverDate ?: java.time.Instant.now())

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: java.time.Instant,
    ) {
        observed += variant
        if (variant == LlmKeyVariant.Credit) flow.value = flow.value.copy(creditExhaustedSeen = true)
    }

    override fun noteWorked(at: java.time.Instant) {
        flow.value = flow.value.copy(creditExhaustedSeen = false)
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class MissionViewModelTest {
    private fun mission(
        version: Int,
        status: MissionStatus = MissionStatus.Draft,
    ) = Mission(
        "t1",
        version,
        status,
        "Perché",
        listOf("a"),
        emptyList(),
        emptyList(),
        Instant.EPOCH,
        Instant.EPOCH.takeIf { status == MissionStatus.Confirmed },
        null,
    )

    private fun pref(
        id: String,
        scope: PreferenceScope = PreferenceScope.Topic,
    ) = Preference(id, scope, "t1".takeIf { scope == PreferenceScope.Topic }, "testo $id", Instant.EPOCH)

    private class FakeMissions : MissionRepository {
        var mission: ApiResult<TopicMission> = ApiResult.Success(TopicMission(null, null))
        var confirmResult: ApiResult<TopicMission> = ApiResult.Success(TopicMission(null, null))
        var versions: ApiResult<List<Mission>> = ApiResult.Success(emptyList())
        var prefs: ApiResult<List<Preference>> = ApiResult.Success(emptyList())
        var deleteResult: ApiResult<Unit> = ApiResult.Success(Unit)
        val confirmed = mutableListOf<Int>()
        val deleted = mutableListOf<String>()
        var getCalls = 0
        var versionCalls = 0

        override suspend fun getMission(topicId: String): ApiResult<TopicMission> {
            getCalls++
            return mission
        }

        override suspend fun listVersions(topicId: String): ApiResult<List<Mission>> {
            versionCalls++
            return versions
        }

        override suspend fun confirm(
            topicId: String,
            version: Int,
        ): ApiResult<TopicMission> {
            confirmed += version
            return confirmResult
        }

        var prefsGate: kotlinx.coroutines.CompletableDeferred<Unit>? = null

        override suspend fun listPreferences(topicId: String): ApiResult<List<Preference>> {
            val snapshot = prefs
            prefsGate?.await()
            return snapshot
        }

        override suspend fun deletePreference(id: String): ApiResult<Unit> {
            deleted += id
            return deleteResult
        }
    }

    private val repo = FakeMissions()
    private val network = ApiFailure.Network(UnreachableReason.Timeout)

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private suspend fun TestScope.started(
        mission: TopicMission = TopicMission(null, null),
        prefs: List<Preference> = emptyList(),
    ): MissionViewModel {
        repo.mission = ApiResult.Success(mission)
        repo.prefs = ApiResult.Success(prefs)
        val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
        vm.onStart()
        advanceUntilIdle()
        return vm
    }

    @Test
    fun `loads the mission and the preferences independently`() =
        runTest {
            repo.mission = ApiResult.Failure(network)
            repo.prefs = ApiResult.Success(listOf(pref("p1")))
            val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
            vm.onStart()
            advanceUntilIdle()
            assertTrue(vm.state.value.load is MissionLoad.Failed)
            assertEquals(PreferencesLoad.Ready, vm.state.value.prefsLoad)
            repo.mission = ApiResult.Success(TopicMission(null, mission(1)))
            vm.retryMission()
            advanceUntilIdle()
            assertEquals(MissionLoad.Ready, vm.state.value.load)
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
        }

    @Test
    fun `a failed refresh keeps what is on screen`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.mission = ApiResult.Failure(network)
            vm.onStart()
            advanceUntilIdle()
            assertEquals(MissionLoad.Ready, vm.state.value.load)
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
        }

    @Test
    fun `confirm ok makes the draft the current mission`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Success(TopicMission(mission(1, MissionStatus.Confirmed), null))
            vm.confirm(1)
            assertEquals(ConfirmState.Confirming(1), vm.state.value.confirm)
            advanceUntilIdle()
            assertEquals(listOf(1), repo.confirmed)
            assertEquals(ConfirmState.Idle, vm.state.value.confirm)
            assertEquals(
                1,
                vm.state.value.current
                    ?.version,
            )
            assertNull(vm.state.value.draft)
        }

    @Test
    fun `a confirmation refused for the key leaves the proposal open with a key notice`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(ApiFailure.LlmKeyMissing)

            vm.confirm(1)
            advanceUntilIdle()

            val failed = vm.state.value.confirm as ConfirmState.Failed
            assertEquals(LlmKeyVariant.Missing, failed.keyNotice?.variant)
            // The card is still the open proposal (with the notice in the error slot), never "confirmed".
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
            assertNull(vm.state.value.current)
            val card = missionCardState(mission(1), vm.state.value.mission, turnBusy = false, confirm = failed)
            assertEquals(MissionCardState.ConfirmError, card)
        }

    @Test
    fun `a confirmation refused for the credit is a credit notice, and a second refusal is a repeat`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(ApiFailure.LlmCreditExhausted)

            vm.confirm(1)
            advanceUntilIdle()
            val first = (vm.state.value.confirm as ConfirmState.Failed).keyNotice!!
            assertEquals(LlmKeyVariant.Credit, first.variant)
            assertFalse(first.repeated)
            assertNull(vm.state.value.current)

            // "Riprova" confirms again with the same proposal.
            vm.confirm(1)
            advanceUntilIdle()
            val second = (vm.state.value.confirm as ConfirmState.Failed).keyNotice!!
            assertTrue(second.repeated)
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
        }

    @Test
    fun `a confirmation refused for the key is a notice and is told to the monitor, once per refusal`() =
        runTest {
            val keys = CreditTrackingMonitor()
            repo.mission = ApiResult.Success(TopicMission(null, mission(1)))
            repo.prefs = ApiResult.Success(emptyList())
            val vm = MissionViewModel("t1", repo, keys)
            vm.onStart()
            advanceUntilIdle()

            repo.confirmResult = ApiResult.Failure(ApiFailure.LlmKeyRejected)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(LlmKeyVariant.Rejected, (vm.state.value.confirm as ConfirmState.Failed).keyNotice?.variant)

            repo.confirmResult = ApiResult.Failure(ApiFailure.LlmKeyMissing)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(LlmKeyVariant.Missing, (vm.state.value.confirm as ConfirmState.Failed).keyNotice?.variant)
            assertEquals(listOf(LlmKeyVariant.Rejected, LlmKeyVariant.Missing), keys.observed)
        }

    @Test
    fun `an ordinary failure is not a key notice`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(network)

            vm.confirm(1)
            advanceUntilIdle()

            assertNull((vm.state.value.confirm as ConfirmState.Failed).keyNotice)
        }

    @Test
    fun `confirm ignores a version that is not the open draft and double taps`() =
        runTest {
            val vm = started(TopicMission(null, mission(2)))
            vm.confirm(1)
            advanceUntilIdle()
            assertTrue(repo.confirmed.isEmpty())
            vm.confirm(2)
            vm.confirm(2)
            advanceUntilIdle()
            assertEquals(listOf(2), repo.confirmed)
        }

    @Test
    fun `409 reloads, recomputes the card and tells the user`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(ApiFailure.Conflict)
            // Meanwhile a newer proposal replaced the draft.
            repo.mission = ApiResult.Success(TopicMission(null, mission(2)))
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(
                2,
                vm.state.value.draft
                    ?.version,
            )
            assertEquals(ConfirmState.Idle, vm.state.value.confirm)
            assertEquals(listOf<MissionEffect>(MissionEffect.MissionChanged), effects)
            assertEquals(
                MissionCardState.Superseded,
                missionCardState(mission(1), vm.state.value.mission, false, vm.state.value.confirm),
            )
            job.cancel()
        }

    @Test
    fun `409 because it was already confirmed elsewhere shows no toast`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(ApiFailure.Conflict)
            repo.mission = ApiResult.Success(TopicMission(mission(1, MissionStatus.Confirmed), null))
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.confirm(1)
            advanceUntilIdle()
            assertTrue(effects.isEmpty())
            assertEquals(
                MissionCardState.Confirmed,
                missionCardState(mission(1), vm.state.value.mission, false, vm.state.value.confirm),
            )
            job.cancel()
        }

    @Test
    fun `network error keeps the draft and offers a retry`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(network)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(ConfirmState.Failed(1), vm.state.value.confirm)
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
            assertEquals(
                MissionCardState.ConfirmError,
                missionCardState(mission(1), vm.state.value.mission, false, vm.state.value.confirm),
            )
            repo.confirmResult = ApiResult.Success(TopicMission(mission(1, MissionStatus.Confirmed), null))
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(ConfirmState.Idle, vm.state.value.confirm)
            assertEquals(listOf(1, 1), repo.confirmed)
        }

    @Test
    fun `the proposed event sets the draft and ignores replays and older ones`() =
        runTest {
            val vm = started(TopicMission(mission(1, MissionStatus.Confirmed), null))
            vm.onEvent(ChatEvent.MissionProposed(mission(2)))
            assertEquals(
                2,
                vm.state.value.draft
                    ?.version,
            )
            assertEquals(
                1,
                vm.state.value.current
                    ?.version,
            )
            vm.onEvent(ChatEvent.MissionProposed(mission(3)))
            vm.onEvent(ChatEvent.MissionProposed(mission(2)))
            assertEquals(
                3,
                vm.state.value.draft
                    ?.version,
            )
            // The card of version 2 is now superseded.
            assertEquals(
                MissionCardState.Superseded,
                missionCardState(mission(2), vm.state.value.mission, false, ConfirmState.Idle),
            )
        }

    @Test
    fun `the confirmed event clears that draft and keeps a newer one`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            vm.onEvent(ChatEvent.MissionConfirmed(mission(1, MissionStatus.Confirmed)))
            assertEquals(
                1,
                vm.state.value.current
                    ?.version,
            )
            assertNull(vm.state.value.draft)

            vm.onEvent(ChatEvent.MissionProposed(mission(3)))
            vm.onEvent(ChatEvent.MissionConfirmed(mission(2, MissionStatus.Confirmed)))
            assertEquals(
                2,
                vm.state.value.current
                    ?.version,
            )
            assertEquals(
                3,
                vm.state.value.draft
                    ?.version,
            )
            // Replay of an older confirmation changes nothing.
            vm.onEvent(ChatEvent.MissionConfirmed(mission(1, MissionStatus.Confirmed)))
            assertEquals(
                2,
                vm.state.value.current
                    ?.version,
            )
        }

    @Test
    fun `events about another topic are ignored`() =
        runTest {
            val vm = started(TopicMission(null, null))
            vm.onEvent(ChatEvent.MissionProposed(mission(1).copy(topicId = "other")))
            assertNull(vm.state.value.draft)
        }

    @Test
    fun `a stale reload that overlaps a stream event is repeated`() =
        runTest {
            repo.mission = ApiResult.Success(TopicMission(null, null))
            val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
            vm.onStart()
            advanceUntilIdle()
            val before = repo.getCalls
            // The event lands, then a resync asks for a new snapshot.
            vm.onEvent(ChatEvent.MissionProposed(mission(1)))
            repo.mission = ApiResult.Success(TopicMission(null, mission(1)))
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertTrue(repo.getCalls > before)
            assertEquals(
                1,
                vm.state.value.draft
                    ?.version,
            )
        }

    @Test
    fun `unknown and unrelated events do not change anything`() =
        runTest {
            val vm = started(TopicMission(mission(1, MissionStatus.Confirmed), null), listOf(pref("p1")))
            val before = vm.state.value
            vm.onEvent(ChatEvent.MessageDelta("m", 0, "x"))
            vm.onEvent(ChatEvent.PreferenceDeleted("nope", PreferenceScope.Topic, "t1"))
            assertEquals(before, vm.state.value)
        }

    @Test
    fun `the saved preference appears once, with a toast, only if it applies here`() =
        runTest {
            val vm = started(prefs = listOf(pref("p1")))
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.onEvent(ChatEvent.PreferenceSaved(pref("p2", PreferenceScope.Global)))
            vm.onEvent(ChatEvent.PreferenceSaved(pref("p2", PreferenceScope.Global)))
            vm.onEvent(ChatEvent.PreferenceSaved(pref("p3").copy(topicId = "other")))
            advanceUntilIdle()
            assertEquals(
                listOf("p1", "p2"),
                vm.state.value.preferences
                    .map { it.id },
            )
            assertEquals(listOf<MissionEffect>(MissionEffect.PreferenceSaved), effects)
            job.cancel()
        }

    @Test
    fun `a preference deleted elsewhere disappears and closes its dialog`() =
        runTest {
            val vm = started(prefs = listOf(pref("p1"), pref("p2")))
            vm.requestDeletePreference(pref("p1"))
            vm.onEvent(ChatEvent.PreferenceDeleted("p1", PreferenceScope.Topic, "t1"))
            assertEquals(
                listOf("p2"),
                vm.state.value.preferences
                    .map { it.id },
            )
            assertNull(vm.state.value.dialog)
        }

    @Test
    fun `deleting a preference asks first, removes the row and moves the focus`() =
        runTest {
            val vm = started(prefs = listOf(pref("p1"), pref("p2")))
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.requestDeletePreference(pref("p1"))
            assertTrue(repo.deleted.isEmpty())
            vm.confirmDeletePreference()
            advanceUntilIdle()
            assertEquals(listOf("p1"), repo.deleted)
            assertEquals(
                listOf("p2"),
                vm.state.value.preferences
                    .map { it.id },
            )
            assertNull(vm.state.value.dialog)
            assertEquals(listOf(MissionEffect.PreferenceDeleted, MissionEffect.PreferenceRemovedFocus), effects)
            job.cancel()
        }

    @Test
    fun `a failed deletion keeps the dialog with an error, 404 closes it quietly`() =
        runTest {
            val vm = started(prefs = listOf(pref("p1")))
            vm.requestDeletePreference(pref("p1"))
            repo.deleteResult = ApiResult.Failure(network)
            vm.confirmDeletePreference()
            advanceUntilIdle()
            assertTrue(
                vm.state.value.dialog
                    ?.failed == true,
            )
            assertEquals(1, vm.state.value.preferences.size)

            repo.deleteResult = ApiResult.Failure(ApiFailure.NotFound)
            vm.confirmDeletePreference()
            advanceUntilIdle()
            assertNull(vm.state.value.dialog)
            assertTrue(
                vm.state.value.preferences
                    .isEmpty(),
            )
        }

    @Test
    fun `versions load on first open and one row expands at a time`() =
        runTest {
            val vm = started(TopicMission(mission(3, MissionStatus.Confirmed), null))
            repo.versions =
                ApiResult.Success(
                    listOf(
                        mission(3, MissionStatus.Confirmed),
                        mission(2, MissionStatus.Superseded),
                        mission(1, MissionStatus.Superseded),
                    ),
                )
            assertEquals(0, repo.versionCalls)
            vm.toggleVersions()
            advanceUntilIdle()
            assertEquals(1, repo.versionCalls)
            // The current version is not repeated among the previous ones.
            assertEquals(
                listOf(2, 1),
                vm.state.value.previousVersions
                    .map { it.version },
            )
            vm.toggleVersion(2)
            vm.toggleVersion(1)
            assertEquals(1, vm.state.value.expandedVersion)
            vm.toggleVersion(1)
            assertNull(vm.state.value.expandedVersion)
            vm.toggleVersions()
            vm.toggleVersions()
            advanceUntilIdle()
            assertEquals(1, repo.versionCalls)
        }

    @Test
    fun `a failed versions load can be retried`() =
        runTest {
            val vm = started(TopicMission(mission(2, MissionStatus.Confirmed), null))
            repo.versions = ApiResult.Failure(network)
            vm.toggleVersions()
            advanceUntilIdle()
            assertEquals(VersionsLoad.Failed, vm.state.value.versions)
            repo.versions = ApiResult.Success(listOf(mission(1, MissionStatus.Superseded)))
            vm.retryVersions()
            advanceUntilIdle()
            assertFalse(vm.state.value.versions is VersionsLoad.Failed)
            assertEquals(1, vm.state.value.previousVersions.size)
        }

    @Test
    fun `the same version proposed again replaces the draft content`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            vm.onEvent(ChatEvent.MissionProposed(mission(1).copy(why = "Nuovo testo")))
            assertEquals(
                "Nuovo testo",
                vm.state.value.draft
                    ?.why,
            )
        }

    private fun gatedPrefs(initial: List<Preference>): kotlinx.coroutines.CompletableDeferred<Unit> {
        repo.prefs = ApiResult.Success(initial)
        return kotlinx.coroutines.CompletableDeferred<Unit>().also { repo.prefsGate = it }
    }

    @Test
    fun `a preference deleted during the fetch does not come back`() =
        runTest {
            val gate = gatedPrefs(listOf(pref("p1"), pref("p2")))
            val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
            vm.onStart()
            advanceUntilIdle()
            vm.onEvent(ChatEvent.PreferenceDeleted("p1", PreferenceScope.Topic, "t1"))
            gate.complete(Unit)
            advanceUntilIdle()
            assertEquals(
                listOf("p2"),
                vm.state.value.preferences
                    .map { it.id },
            )
        }

    @Test
    fun `a preference saved during the first load is kept`() =
        runTest {
            val gate = gatedPrefs(listOf(pref("p1")))
            val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
            vm.onStart()
            advanceUntilIdle()
            assertEquals(PreferencesLoad.Loading, vm.state.value.prefsLoad)
            vm.onEvent(ChatEvent.PreferenceSaved(pref("p2", PreferenceScope.Global)))
            gate.complete(Unit)
            advanceUntilIdle()
            assertEquals(
                listOf("p1", "p2"),
                vm.state.value.preferences
                    .map { it.id },
            )
        }

    @Test
    fun `a stale fetch does not overwrite a preference saved meanwhile`() =
        runTest {
            val vm = started(prefs = listOf(pref("p1")))
            val gate = gatedPrefs(listOf(pref("p1")))
            vm.onStart()
            advanceUntilIdle()
            vm.onEvent(ChatEvent.PreferenceSaved(pref("p3")))
            gate.complete(Unit)
            advanceUntilIdle()
            assertEquals(
                listOf("p1", "p3"),
                vm.state.value.preferences
                    .map { it.id },
            )
        }

    @Test
    fun `409 with the same draft still open is a plain failure without toast`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Failure(ApiFailure.Conflict)
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(ConfirmState.Failed(1), vm.state.value.confirm)
            assertTrue(effects.isEmpty())
            job.cancel()
        }

    @Test
    fun `a successful confirmation announces itself`() =
        runTest {
            val vm = started(TopicMission(null, mission(1)))
            repo.confirmResult = ApiResult.Success(TopicMission(mission(1, MissionStatus.Confirmed), null))
            val effects = mutableListOf<MissionEffect>()
            val job = backgroundCollect(vm, effects)
            vm.confirm(1)
            advanceUntilIdle()
            assertEquals(listOf<MissionEffect>(MissionEffect.Confirmed), effects)
            job.cancel()
        }

    @Test
    fun `an open versions list reloads on a new proposal and on resync`() =
        runTest {
            val vm = started(TopicMission(mission(2, MissionStatus.Confirmed), null))
            vm.toggleVersions()
            advanceUntilIdle()
            val calls = repo.versionCalls
            vm.onEvent(ChatEvent.MissionProposed(mission(3)))
            advanceUntilIdle()
            assertEquals(calls + 1, repo.versionCalls)
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertEquals(calls + 2, repo.versionCalls)
        }

    @Test
    fun `without a stream signal the mission loads on its own after a grace period`() =
        runTest {
            val vm = MissionViewModel("t1", repo, CreditTrackingMonitor())
            vm.loadIfStreamSilent()
            advanceUntilIdle()
            assertEquals(1, repo.getCalls)
            // With the stream's signal first there is no second fetch.
            val other = MissionViewModel("t1", repo, CreditTrackingMonitor())
            other.loadIfStreamSilent()
            other.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertEquals(2, repo.getCalls)
        }

    private fun TestScope.backgroundCollect(
        vm: MissionViewModel,
        into: MutableList<MissionEffect>,
    ) = backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.effects.collect { into += it } }
}
