package com.teachme.feature.mission

import androidx.compose.runtime.Composable
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AppClock
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.KeyNoticeSource
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LoadError
import com.teachme.core.model.Mission
import com.teachme.core.model.MissionRepository
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.Preference
import com.teachme.core.model.PreferenceScope
import com.teachme.core.model.TopicMission
import com.teachme.core.model.budgetResetAt
import com.teachme.core.model.toLoadError
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface MissionLoad {
    data object Loading : MissionLoad

    data class Failed(
        val error: LoadError,
    ) : MissionLoad

    data object Ready : MissionLoad
}

sealed interface VersionsLoad {
    /** Not requested yet: the list loads the first time the section is opened. */
    data object NotLoaded : VersionsLoad

    data object Loading : VersionsLoad

    data object Failed : VersionsLoad

    data class Loaded(
        val items: List<Mission>,
    ) : VersionsLoad
}

enum class PreferencesLoad { Loading, Failed, Ready }

/** The "Eliminare questa preferenza?" dialog. */
data class PreferenceDialog(
    val preference: Preference,
    val submitting: Boolean = false,
    val failed: Boolean = false,
)

data class MissionUiState(
    val load: MissionLoad = MissionLoad.Loading,
    val mission: TopicMission? = null,
    val confirm: ConfirmState = ConfirmState.Idle,
    val versionsOpen: Boolean = false,
    val versions: VersionsLoad = VersionsLoad.NotLoaded,
    val versionsShown: Int = VERSIONS_PAGE_SIZE,
    val expandedVersion: Int? = null,
    val prefsLoad: PreferencesLoad = PreferencesLoad.Loading,
    val preferences: List<Preference> = emptyList(),
    val dialog: PreferenceDialog? = null,
) {
    val draft: Mission? get() = mission?.draft
    val current: Mission? get() = mission?.current
    val confirming: Boolean get() = confirm is ConfirmState.Confirming

    /** Versions to list in "Versioni precedenti": everything but the current one and the open draft. */
    val previousVersions: List<Mission>
        get() =
            (versions as? VersionsLoad.Loaded)
                ?.items
                .orEmpty()
                .filter { it.version != current?.version && it.version != draft?.version }
                .sortedByDescending { it.version }
}

sealed interface MissionEffect {
    /** A confirmation hit a 409: the proposal changed, the card was recomputed (toast). */
    data object MissionChanged : MissionEffect

    /** The Tutor saved a preference (toast with "Vedi"). */
    data object PreferenceSaved : MissionEffect

    data object PreferenceDeleted : MissionEffect

    /** The user confirmed the mission: announce it and move the focus out of the card that goes away. */
    data object Confirmed : MissionEffect

    /** The preference was deleted: the focus should go to the next row or to the section title. */
    data object PreferenceRemovedFocus : MissionEffect
}

/** Reloads attempted when stream events arrive while a fetch is in flight (the fetch may be stale). */
private const val MAX_STALE_RELOADS = 2

/** How long the stream gets to open (and trigger the first load) before the mission is fetched on its own. */
private const val STREAM_GRACE_MS = 3_000L

/**
 * Mission and preferences of one Topic. Fed by REST (on start, after a conflict, on resync) and by the
 * Topic stream events that the chat forwards through [onEvent]. Events are idempotent: replays and
 * older versions never move the state backwards.
 */
@HiltViewModel(assistedFactory = MissionViewModel.Factory::class)
class MissionViewModel
    @AssistedInject
    constructor(
        @Assisted private val topicId: String,
        private val missions: MissionRepository,
        /** Told when a request meets an exhausted credit (the key notice is born here, not in the panel). */
        private val llmKey: LlmKeyMonitor = NoLlmKeyMonitor,
        private val clock: AppClock = AppClock { System.currentTimeMillis() },
    ) : ViewModel() {
        @AssistedFactory
        interface Factory {
            fun create(topicId: String): MissionViewModel
        }

        private val _state = MutableStateFlow(MissionUiState())
        val state: StateFlow<MissionUiState> = _state.asStateFlow()

        private val effectChannel = Channel<MissionEffect>(Channel.BUFFERED)
        val effects: Flow<MissionEffect> = effectChannel.receiveAsFlow()

        private var fetchedOnce = false
        private var missionJob: Job? = null
        private var prefsJob: Job? = null
        private var versionsJob: Job? = null

        /** Counts stream events that changed the state: a fetch that started before one may be stale. */
        private var eventCounter = 0

        /** The screen became visible (also coming back to the foreground): refresh both sections. */
        fun onStart() {
            fetchedOnce = true
            loadMission()
            loadPreferences()
            if (_state.value.versionsOpen) loadVersions()
        }

        /**
         * Normally the first load is triggered by the stream opening (the chat forwards it as a resync). If that never
         * happens (the stream cannot open) this loads anyway after a short wait, so the tab does not stay empty.
         */
        fun loadIfStreamSilent() {
            if (fetchedOnce) return
            viewModelScope.launch {
                delay(STREAM_GRACE_MS)
                if (!fetchedOnce) onStart()
            }
        }

        fun retryMission() {
            _state.update { it.copy(load = MissionLoad.Loading) }
            loadMission()
        }

        fun retryPreferences() {
            _state.update { it.copy(prefsLoad = PreferencesLoad.Loading) }
            loadPreferences()
        }

        /** GET mission, repeated (twice at most) when a stream event or a confirmation landed while it was in flight. */
        private suspend fun fetchMission(): ApiResult<TopicMission> {
            var attempts = 0
            while (true) {
                val epoch = eventCounter
                val result = missions.getMission(topicId)
                if (result is ApiResult.Success && epoch != eventCounter && attempts++ < MAX_STALE_RELOADS) continue
                return result
            }
        }

        private fun loadMission() {
            missionJob?.cancel()
            missionJob =
                viewModelScope.launch {
                    when (val result = fetchMission()) {
                        is ApiResult.Success ->
                            _state.update {
                                it.copy(
                                    load = MissionLoad.Ready,
                                    mission = result.value,
                                    confirm = it.confirm.settledBy(result.value),
                                )
                            }
                        is ApiResult.Failure ->
                            // A failed refresh keeps what is on screen; only the first load shows an error.
                            if (_state.value.load != MissionLoad.Ready) {
                                _state.update { it.copy(load = MissionLoad.Failed(result.failure.toLoadError())) }
                            }
                    }
                }
        }

        /**
         * Preference events that arrive while the list is being fetched are kept here and re-applied on top of the
         * result, so a stale fetch never undoes a deletion or drops a saved preference.
         */
        private val prefsBuffer = mutableListOf<ChatEvent>()
        private var prefsInFlight = false

        private fun loadPreferences() {
            prefsJob?.cancel()
            prefsBuffer.clear()
            prefsInFlight = true
            prefsJob =
                viewModelScope.launch {
                    when (val result = missions.listPreferences(topicId)) {
                        is ApiResult.Success -> {
                            val buffered = prefsBuffer.toList()
                            prefsBuffer.clear()
                            prefsInFlight = false
                            _state.update { it.copy(prefsLoad = PreferencesLoad.Ready, preferences = result.value) }
                            buffered.forEach { applyPreferenceEvent(it, toast = false) }
                        }
                        is ApiResult.Failure -> {
                            prefsBuffer.clear()
                            prefsInFlight = false
                            if (_state.value.prefsLoad != PreferencesLoad.Ready) {
                                _state.update { it.copy(prefsLoad = PreferencesLoad.Failed) }
                            }
                        }
                    }
                }
        }

        private fun applyPreferenceEvent(
            event: ChatEvent,
            toast: Boolean,
        ) {
            when (event) {
                is ChatEvent.PreferenceSaved -> {
                    val pref = event.preference
                    if (pref.scope != PreferenceScope.Global && pref.topicId != topicId) return
                    var added = false
                    _state.update { s ->
                        if (s.prefsLoad != PreferencesLoad.Ready || s.preferences.any { it.id == pref.id }) {
                            return@update s
                        }
                        added = true
                        s.copy(preferences = s.preferences + pref)
                    }
                    if (added && toast) effectChannel.trySend(MissionEffect.PreferenceSaved)
                }
                is ChatEvent.PreferenceDeleted -> removePreference(event.preferenceId)
                else -> Unit
            }
        }

        /** An event of the Topic stream (forwarded by the chat). Events of other types are ignored. */
        fun onEvent(event: ChatEvent) {
            when (event) {
                is ChatEvent.MissionProposed -> {
                    if (event.mission.topicId != topicId) return
                    eventCounter++
                    _state.update { s ->
                        val base = s.mission ?: return@update s
                        val draft = base.draft
                        // Replays and older proposals never replace a newer draft.
                        // Same version again (a retried message): upsert; older ones never replace a newer draft.
                        if (draft != null && draft.version > event.mission.version) return@update s
                        if ((base.current?.version ?: 0) >= event.mission.version) return@update s
                        s.copy(mission = base.copy(draft = event.mission))
                    }
                    if (_state.value.versionsOpen) loadVersions()
                }
                is ChatEvent.MissionConfirmed -> {
                    if (event.mission.topicId != topicId) return
                    eventCounter++
                    _state.update { s ->
                        val base = s.mission ?: return@update s
                        if ((base.current?.version ?: 0) >= event.mission.version) return@update s
                        val draft = base.draft?.takeIf { it.version > event.mission.version }
                        val next = TopicMission(event.mission, draft)
                        s.copy(
                            mission = next,
                            confirm =
                                if (s.confirm.versionOrNull() ==
                                    event.mission.version
                                ) {
                                    ConfirmState.Idle
                                } else {
                                    s.confirm
                                },
                            versions = if (s.versionsOpen) VersionsLoad.Loading else VersionsLoad.NotLoaded,
                        )
                    }
                    if (_state.value.versionsOpen) loadVersions()
                }
                is ChatEvent.PreferenceSaved, is ChatEvent.PreferenceDeleted -> {
                    if (prefsInFlight) {
                        prefsBuffer += event
                        // A deletion also applies at once: the row must not come back with the stale list.
                        if (event is ChatEvent.PreferenceDeleted) removePreference(event.preferenceId)
                    } else {
                        applyPreferenceEvent(event, toast = true)
                    }
                }
                ChatEvent.ResyncRequired -> onStart()
                else -> Unit
            }
        }

        private val notices = KeyNoticeSource(llmKey)

        /** Confirms [version] if it is the open draft. The result decides the card state (never the click). */
        fun confirm(version: Int) {
            val s = _state.value
            if (s.confirming || s.draft?.version != version) return
            _state.update { it.copy(confirm = ConfirmState.Confirming(version)) }
            viewModelScope.launch {
                when (val result = missions.confirm(topicId, version)) {
                    is ApiResult.Success -> {
                        // A GET that started before this answer must not bring the card back to "open".
                        eventCounter++
                        _state.update {
                            it.copy(
                                mission = result.value,
                                load = MissionLoad.Ready,
                                confirm = ConfirmState.Idle,
                                versions = if (it.versionsOpen) VersionsLoad.Loading else VersionsLoad.NotLoaded,
                            )
                        }
                        if (_state.value.versionsOpen) loadVersions()
                        effectChannel.send(MissionEffect.Confirmed)
                    }
                    is ApiResult.Failure ->
                        if (result.failure == ApiFailure.Conflict || result.failure == ApiFailure.NotFound) {
                            onConflict(version)
                        } else {
                            _state.update {
                                it.copy(
                                    confirm =
                                        ConfirmState.Failed(
                                            version,
                                            notices.of(result),
                                            result.failure.budgetResetAt(clock.nowMs()),
                                        ),
                                )
                            }
                        }
                }
            }
        }

        /**
         * 409: reload and recompute the card. If the same draft is still the open one (state-related conflict) it is a
         * plain failure with "Riprova"; if it became the current mission there is nothing to say; otherwise toast.
         */
        private suspend fun onConflict(version: Int) {
            when (val result = fetchMission()) {
                is ApiResult.Success -> {
                    val mission = result.value
                    val stillOpen = mission.draft?.version == version && mission.current?.version != version
                    _state.update {
                        it.copy(
                            mission = mission,
                            confirm = if (stillOpen) ConfirmState.Failed(version) else ConfirmState.Idle,
                        )
                    }
                    if (!stillOpen && mission.current?.version != version) {
                        effectChannel.send(MissionEffect.MissionChanged)
                    }
                }
                is ApiResult.Failure -> _state.update { it.copy(confirm = ConfirmState.Failed(version)) }
            }
        }

        // Versions

        fun toggleVersions() {
            val open = !_state.value.versionsOpen
            _state.update { it.copy(versionsOpen = open) }
            if (open && _state.value.versions.let { it is VersionsLoad.NotLoaded || it is VersionsLoad.Failed }) {
                _state.update { it.copy(versions = VersionsLoad.Loading) }
                loadVersions()
            }
        }

        fun retryVersions() {
            _state.update { it.copy(versions = VersionsLoad.Loading) }
            loadVersions()
        }

        private fun loadVersions() {
            versionsJob?.cancel()
            versionsJob =
                viewModelScope.launch {
                    when (val result = missions.listVersions(topicId)) {
                        is ApiResult.Success ->
                            _state.update { it.copy(versions = VersionsLoad.Loaded(result.value)) }
                        is ApiResult.Failure -> _state.update { it.copy(versions = VersionsLoad.Failed) }
                    }
                }
        }

        /** One version open at a time (mission.md design section 4.3). */
        fun toggleVersion(version: Int) =
            _state.update { it.copy(expandedVersion = if (it.expandedVersion == version) null else version) }

        fun showMoreVersions() = _state.update { it.copy(versionsShown = it.versionsShown + VERSIONS_PAGE_SIZE) }

        // Preferences

        fun requestDeletePreference(preference: Preference) =
            _state.update { it.copy(dialog = PreferenceDialog(preference)) }

        fun dismissDeleteDialog() {
            if (_state.value.dialog?.submitting == true) return
            _state.update { it.copy(dialog = null) }
        }

        fun confirmDeletePreference() {
            val dialog = _state.value.dialog ?: return
            if (dialog.submitting) return
            _state.update { it.copy(dialog = dialog.copy(submitting = true, failed = false)) }
            viewModelScope.launch {
                when (val result = missions.deletePreference(dialog.preference.id)) {
                    is ApiResult.Success -> {
                        removePreference(dialog.preference.id)
                        _state.update { it.copy(dialog = null) }
                        effectChannel.send(MissionEffect.PreferenceDeleted)
                        effectChannel.send(MissionEffect.PreferenceRemovedFocus)
                    }
                    is ApiResult.Failure ->
                        if (result.failure == ApiFailure.NotFound) {
                            // Already gone elsewhere: close and drop the row without a word.
                            removePreference(dialog.preference.id)
                            _state.update { it.copy(dialog = null) }
                        } else {
                            _state.update { it.copy(dialog = dialog.copy(submitting = false, failed = true)) }
                        }
                }
            }
        }

        private fun removePreference(id: String) {
            _state.update { s ->
                s.copy(
                    preferences = s.preferences.filterNot { it.id == id },
                    // The dialog of a preference deleted elsewhere closes by itself.
                    dialog = s.dialog?.takeUnless { it.preference.id == id && !it.submitting },
                )
            }
        }
    }

/** The confirmation settles when the reloaded data shows the outcome. */
private fun ConfirmState.settledBy(mission: TopicMission): ConfirmState =
    when (this) {
        is ConfirmState.Confirming -> this
        is ConfirmState.Failed -> if (mission.draft?.version == version) this else ConfirmState.Idle
        ConfirmState.Idle -> this
    }

private fun ConfirmState.versionOrNull(): Int? =
    when (this) {
        is ConfirmState.Confirming -> version
        is ConfirmState.Failed -> version
        ConfirmState.Idle -> null
    }

/** The ViewModel of the mission of [topicId]; the Topic screen shares it between the chat card and the tab. */
@Composable
fun rememberMissionViewModel(topicId: String): MissionViewModel =
    hiltViewModel<MissionViewModel, MissionViewModel.Factory>(key = "mission-$topicId") { it.create(topicId) }
