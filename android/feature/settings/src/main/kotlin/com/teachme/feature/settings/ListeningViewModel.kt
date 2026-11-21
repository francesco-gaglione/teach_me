package com.teachme.feature.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioPreferences
import com.teachme.core.model.AudioRepository
import com.teachme.core.model.AuthState
import com.teachme.core.model.ClipPlayer
import com.teachme.core.model.ClipPlayerFactory
import com.teachme.core.model.PlaybackEnd
import com.teachme.core.model.PlaybackSpeed
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.TtsVoice
import com.teachme.core.network.AuthApi
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/** What the "Prova" button of a voice shows (lesson-audio.md section 4). */
enum class SampleState {
    /** A clip exists: "Prova". */
    Ready,

    /** The clip is playing: "Ferma". */
    Playing,

    /** `sample_url` is null: "In preparazione", disabled. */
    Preparing,

    /** The clip could not be played: "Non disponibile", disabled. */
    Unavailable,
}

data class VoiceRowUi(
    val id: String,
    val label: String,
    val sample: SampleState,
)

/**
 * State of the "Ascolto" section. [visible] is false until the voice list has arrived, and stays false when the feature
 * is off or the request failed: the section is then not drawn at all.
 */
data class ListeningUiState(
    val visible: Boolean = false,
    val speed: PlaybackSpeed = PlaybackSpeed.Default,
    val voices: List<VoiceRowUi> = emptyList(),
    /** The voice the radio shows: the one being saved while a request is in flight, else the confirmed one. */
    val selectedVoice: String? = null,
    /** The voice a request is saving, if any: its row says "Salvataggio…". */
    val savingVoice: String? = null,
)

/** One-shot events for the screen. */
sealed interface ListeningEffect {
    /** Saving the voice failed: toast "Non sono riuscito a salvare la voce. Riprova.". */
    data object VoiceSaveFailed : ListeningEffect
}

/**
 * The "Ascolto" section of Settings (lesson-audio.md section 4): reading speed (a device preference), the voice of the
 * account and the samples. The voice list is asked again every time the screen starts.
 */
@HiltViewModel
class ListeningViewModel
    @Inject
    constructor(
        private val audio: AudioRepository,
        private val preferences: AudioPreferences,
        private val auth: AuthApi,
        private val sessions: SessionRepository,
        private val playerFactory: ClipPlayerFactory,
    ) : ViewModel() {
        private data class Loaded(
            val voices: List<TtsVoice>,
            val defaultVoice: String?,
        )

        private val _state = MutableStateFlow(ListeningUiState())
        val state: StateFlow<ListeningUiState> = _state.asStateFlow()

        private val effectChannel = Channel<ListeningEffect>(Channel.BUFFERED)
        val effects: Flow<ListeningEffect> = effectChannel.receiveAsFlow()

        private var loaded: Loaded? = null
        private var confirmed: String? = null
        private var playing: String? = null
        private val unavailable = mutableSetOf<String>()
        private var loadJob: Job? = null
        private var saveJob: Job? = null
        private var player: ClipPlayer? = null

        init {
            viewModelScope.launch {
                preferences.speed.collect { speed -> _state.update { it.copy(speed = speed) } }
            }
            viewModelScope.launch {
                sessions.state.collect { auth ->
                    confirmed = (auth as? AuthState.SignedIn)?.session?.user?.ttsVoice
                    publish()
                }
            }
        }

        /** The screen came to the foreground: the list is asked again, and a sample that failed gets another chance. */
        fun onStarted() {
            loadJob?.cancel()
            loadJob =
                viewModelScope.launch {
                    unavailable.clear()
                    when (val result = audio.getVoices()) {
                        is ApiResult.Success -> {
                            val v = result.value
                            loaded =
                                if (v.available && v.voices.isNotEmpty()) Loaded(v.voices, v.defaultVoice) else null
                        }
                        is ApiResult.Failure -> loaded = null
                    }
                    if (loaded == null) stopSample()
                    publish()
                }
        }

        /** Leaving Settings (a sub-screen, another app, the screen off): no sample keeps playing. */
        fun onStopped() = stopSample()

        fun onSpeedSelected(speed: PlaybackSpeed) {
            viewModelScope.launch { preferences.setSpeed(speed) }
        }

        // region Voice

        /** A tap on a row. Optimistic: the radio moves at once and goes back if the server refuses. */
        fun onVoiceSelected(id: String) {
            val current = loaded ?: return
            if (saveJob?.isActive == true || current.voices.none { it.id == id }) return
            if (id == effectiveConfirmed()) return
            _state.update { it.copy(selectedVoice = id, savingVoice = id) }
            saveJob =
                viewModelScope.launch {
                    when (val result = auth.setTtsVoice(id)) {
                        is ApiResult.Success -> {
                            confirmed = result.value.ttsVoice ?: id
                            sessions.updateUser(result.value)
                            _state.update { it.copy(savingVoice = null) }
                            publish()
                        }
                        is ApiResult.Failure -> {
                            _state.update { it.copy(savingVoice = null) }
                            publish()
                            // A 401 is the global handler's: the screen stays quiet.
                            if (result.failure != ApiFailure.Unauthorized) {
                                effectChannel.send(ListeningEffect.VoiceSaveFailed)
                            }
                            // The list or the feature changed on the server: look again.
                            if (result.failure is ApiFailure.Validation || result.failure == ApiFailure.Conflict) {
                                onStarted()
                            }
                        }
                    }
                }
        }

        private fun effectiveConfirmed(): String? {
            val current = loaded ?: return null
            return confirmed?.takeIf { id -> current.voices.any { it.id == id } } ?: current.defaultVoice
        }

        // endregion

        // region Samples

        /** "Prova" / "Ferma" of a row: never selects the voice, always at 1x, one sample at a time. */
        fun onSampleClicked(id: String) {
            val voice = loaded?.voices?.firstOrNull { it.id == id } ?: return
            val url = voice.sampleUrl ?: return
            if (id in unavailable) return
            if (playing == id) {
                stopSample()
                return
            }
            val p = player ?: playerFactory.create().also { player = it }
            // The speed preference is for lessons; a voice is judged at its natural pace.
            p.setSpeed(PlaybackSpeed.X1.factor)
            playing = id
            p.play(url) { end -> onSampleEnded(id, end) }
            publish()
        }

        private fun onSampleEnded(
            id: String,
            end: PlaybackEnd,
        ) {
            if (playing != id) return
            playing = null
            if (end == PlaybackEnd.Error) unavailable += id
            publish()
        }

        private fun stopSample() {
            player?.stop()
            if (playing != null) {
                playing = null
                publish()
            }
        }

        // endregion

        private fun publish() {
            val current = loaded
            _state.update { s ->
                if (current == null) {
                    s.copy(visible = false, voices = emptyList(), selectedVoice = null, savingVoice = null)
                } else {
                    s.copy(
                        visible = true,
                        voices =
                            current.voices.map { v ->
                                VoiceRowUi(
                                    v.id,
                                    v.label,
                                    when {
                                        v.id == playing -> SampleState.Playing
                                        v.sampleUrl == null -> SampleState.Preparing
                                        v.id in unavailable -> SampleState.Unavailable
                                        else -> SampleState.Ready
                                    },
                                )
                            },
                        selectedVoice = s.savingVoice ?: effectiveConfirmed(),
                    )
                }
            }
        }

        override fun onCleared() {
            player?.release()
            player = null
        }
    }
