package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioManifest
import com.teachme.core.model.AudioPreferences
import com.teachme.core.model.AudioRepository
import com.teachme.core.model.AudioStatus
import com.teachme.core.model.BlockAudio
import com.teachme.core.model.ClipPlayer
import com.teachme.core.model.ClipPlayerFactory
import com.teachme.core.model.PlaybackEnd
import com.teachme.core.model.PlaybackSpeed
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Interval of the manifest refresh while some block is `pending` (audio.md section 3). Tests shorten it. */
open class AudioTimings
    @Inject
    constructor() {
        open val manifestPollMs: Long = MANIFEST_POLL_MS
    }

private const val MANIFEST_POLL_MS = 15_000L
private const val FIXTURE_PREFIX = "fixture/"
private const val EVENT_BUFFER = 8
private const val MILLIS_PER_SECOND = 1_000L

/** The "Ascolta" control of one block (audio.md section 6). A block outside the manifest has none. */
sealed interface BlockListenState {
    /** A clip can be played (its own voice's, or another voice's as a fallback). [playing]: it is being read now. */
    data class Playable(
        val clipUrl: String,
        val playing: Boolean,
    ) : BlockListenState

    /** Disabled, "In preparazione". */
    data object Preparing : BlockListenState

    /** Disabled, "Audio non disponibile". */
    data object Unavailable : BlockListenState
}

/** Pure mapping of one manifest entry (audio.md section 6). */
fun BlockAudio.listenState(playing: Boolean = false): BlockListenState {
    val playable = clip
    return when {
        playable != null -> BlockListenState.Playable(playable.url, playing)
        status == AudioStatus.Failed -> BlockListenState.Unavailable
        else -> BlockListenState.Preparing
    }
}

data class LessonAudioUiState(
    /** False when the feature is off or unknown: no listening control is shown. */
    val available: Boolean = false,
    val blocks: Map<String, BlockAudio> = emptyMap(),
    /** Id of the block being read, or null. */
    val playingBlockId: String? = null,
    /** True while "Ascolta la lezione" runs: the audio strip is shown. A single block reading leaves it false. */
    val continuous: Boolean = false,
    val speed: PlaybackSpeed = PlaybackSpeed.Default,
) {
    /** "Ascolta la lezione" works as soon as one block has a clip; until then it is shown disabled. */
    val canListenToLesson: Boolean get() = available && blocks.values.any { it.clip != null }

    /** What to draw under [blockId], or null for no control at all. */
    fun listenState(blockId: String): BlockListenState? =
        if (!available) null else blocks[blockId]?.listenState(playing = blockId == playingBlockId)
}

/** What the screen has to do for the continuous reading (the view model knows nothing of the list). */
sealed interface LessonAudioEvent {
    /** A block of the continuous reading starts: bring it into view if it is not fully visible. */
    data class Reading(
        val blockId: String,
    ) : LessonAudioEvent

    /** The reading stopped before a required exercise nobody answered: scroll to it and say why. */
    data class StoppedAtExercise(
        val blockId: String,
    ) : LessonAudioEvent

    /** The last revealed block was read: the page goes to "Continua" if it is not in view. */
    data object Finished : LessonAudioEvent

    /** A clip could not be played (single block or continuous reading): the screen shows a toast. */
    data object PlaybackFailed : LessonAudioEvent
}

/**
 * Audio of the open lesson: the manifest (refreshed every 15 s while a block is pending), the single player and the
 * stop rules (leaving the lesson, another block, app in background). Lives as long as the lesson screen's back stack
 * entry; the player is created on the first play and released in [onCleared].
 */
@HiltViewModel
class LessonAudioViewModel
    @Inject
    constructor(
        handle: SavedStateHandle,
        private val audio: AudioRepository,
        private val playerFactory: ClipPlayerFactory,
        private val timings: AudioTimings,
        private val preferences: AudioPreferences,
    ) : ViewModel() {
        private val key: String = handle.get<String>("key").orEmpty()

        private val _state = MutableStateFlow(LessonAudioUiState())
        val state: StateFlow<LessonAudioUiState> = _state.asStateFlow()

        private val _events = MutableSharedFlow<LessonAudioEvent>(extraBufferCapacity = EVENT_BUFFER)
        val events: SharedFlow<LessonAudioEvent> = _events.asSharedFlow()

        private var player: ClipPlayer? = null
        private var polling: Job? = null

        /** Saves of the speed not finished yet: while any is in flight the stored flow may still hold older values. */
        private var speedSavesInFlight = 0
        private var chosenSpeed: PlaybackSpeed? = null

        /** The slots of the continuous reading, asked again at every step: answers and revealed sections change. */
        private var slotSource: (() -> List<ReadingSlot>)? = null
        private var playedAny = false

        init {
            viewModelScope.launch {
                preferences.speed.collect { if (speedSavesInFlight == 0) applySpeed(it) }
            }
            onStarted()
        }

        /** The screen is visible (ON_START): polling runs, if it is not running yet. Sample lessons are not on the backend. */
        fun onStarted() {
            if (key.isEmpty() || key.startsWith(FIXTURE_PREFIX)) return
            if (polling?.isActive == true) return
            startPolling()
        }

        /** The screen left the foreground (ON_STOP): no request in the background or while the lesson is in the back stack. */
        fun onStopped() {
            polling?.cancel()
            polling = null
        }

        private fun startPolling() {
            polling =
                viewModelScope.launch {
                    // First the feature switch; a feature that is off stops everything.
                    while (true) {
                        val wait =
                            when (val voices = audio.getVoices()) {
                                is ApiResult.Success -> {
                                    if (!voices.value.available) return@launch
                                    break
                                }
                                is ApiResult.Failure -> retryDelay(voices.failure) ?: return@launch
                            }
                        delay(wait)
                    }
                    while (true) {
                        val wait = refreshManifest() ?: return@launch
                        delay(wait)
                    }
                }
        }

        /** Returns the time to wait before the next refresh, or null when polling is over. */
        private suspend fun refreshManifest(): Long? =
            when (val result = audio.getManifest(key)) {
                is ApiResult.Success -> {
                    apply(result.value)
                    if (result.value.hasPending) timings.manifestPollMs else null
                }
                is ApiResult.Failure -> retryDelay(result.failure)
            }

        /** Every failure is tried again later except the ones a retry cannot fix; a 429 waits for `Retry-After`. */
        private fun retryDelay(failure: ApiFailure): Long? =
            when (failure) {
                ApiFailure.Unauthorized, ApiFailure.NotFound, ApiFailure.InvalidId -> null
                is ApiFailure.RateLimited ->
                    maxOf(timings.manifestPollMs, (failure.retryAfterSecs ?: 0) * MILLIS_PER_SECOND)
                else -> timings.manifestPollMs
            }

        private fun apply(manifest: AudioManifest) {
            _state.value =
                _state.value.copy(
                    available = manifest.blocks.isNotEmpty(),
                    blocks = manifest.blocks.associateBy { it.blockId },
                )
        }

        /** The button of [blockId]: stops it when it is the one being read, otherwise starts it (stopping any other). */
        fun toggle(blockId: String) {
            val current = _state.value
            if (current.playingBlockId == blockId) {
                stop()
                return
            }
            val button = current.listenState(blockId) as? BlockListenState.Playable ?: return
            slotSource = null
            play(blockId, button.clipUrl, continuous = false)
        }

        private fun play(
            blockId: String,
            clipUrl: String,
            continuous: Boolean,
        ) {
            val exo = ensurePlayer()
            _state.value = _state.value.copy(playingBlockId = blockId, continuous = continuous)
            exo.play(clipUrl) { end ->
                // Only the end of the clip still being read counts.
                val s = _state.value
                when {
                    s.playingBlockId != blockId -> Unit
                    // A clip that fails ends the reading: with the backend unreachable every next one would fail too.
                    s.continuous && slotSource != null ->
                        if (end == PlaybackEnd.Completed) {
                            readAfter(blockId)
                        } else {
                            endReading(finished = false)
                            _events.tryEmit(LessonAudioEvent.PlaybackFailed)
                        }
                    else -> {
                        _state.value = s.copy(playingBlockId = null)
                        if (end == PlaybackEnd.Error) _events.tryEmit(LessonAudioEvent.PlaybackFailed)
                    }
                }
            }
        }

        private fun ensurePlayer(): ClipPlayer =
            player ?: playerFactory.create().also {
                player = it
                it.setSpeed(_state.value.speed.factor)
            }

        /**
         * "Ascolta la lezione": reads from block [startId] to the end of the revealed sections, in document order. [slots] is
         * called at every step. Blocks without a clip are skipped, exercises are never read, and the reading stops before the
         * first required exercise not answered yet. Replaces whatever is playing.
         */
        fun listenToLesson(
            startId: String,
            slots: () -> List<ReadingSlot>,
        ) {
            if (!_state.value.canListenToLesson) return
            slotSource = slots
            playedAny = false
            val start = slots().indexOfFirst { it.blockId == startId }
            if (start < 0) endReading(finished = false) else readFrom(start)
        }

        private fun readAfter(blockId: String) {
            val slots = slotSource?.invoke().orEmpty()
            val at = slots.indexOfFirst { it.blockId == blockId }
            if (at < 0) endReading(finished = true) else readFrom(at + 1)
        }

        private fun readFrom(from: Int) {
            val slots = slotSource?.invoke().orEmpty()
            for (index in from until slots.size) {
                when (val slot = slots[index]) {
                    is ReadingSlot.Gate -> {
                        endReading(finished = false)
                        _events.tryEmit(LessonAudioEvent.StoppedAtExercise(slot.blockId))
                        return
                    }
                    is ReadingSlot.Read -> {
                        val button = _state.value.listenState(slot.blockId) as? BlockListenState.Playable ?: continue
                        playedAny = true
                        _events.tryEmit(LessonAudioEvent.Reading(slot.blockId))
                        play(slot.blockId, button.clipUrl, continuous = true)
                        return
                    }
                }
            }
            endReading(finished = true)
        }

        /** The continuous reading is over: [finished] when it reached the last revealed block after reading something. */
        private fun endReading(finished: Boolean) {
            val reachedEnd = finished && playedAny
            slotSource = null
            playedAny = false
            player?.stop()
            _state.value = _state.value.copy(playingBlockId = null, continuous = false)
            if (reachedEnd) _events.tryEmit(LessonAudioEvent.Finished)
        }

        /** The speed chip: next step of 0,75 / 1 / 1,25 / 1,5 (then back), live on the clip in progress, kept on the device. */
        fun cycleSpeed() {
            val next = _state.value.speed.next()
            applySpeed(next)
            chosenSpeed = next
            speedSavesInFlight++
            viewModelScope.launch {
                try {
                    preferences.setSpeed(next)
                } finally {
                    speedSavesInFlight--
                    // The flow was ignored meanwhile: the last tap wins.
                    if (speedSavesInFlight == 0) chosenSpeed?.let(::applySpeed)
                }
            }
        }

        private fun applySpeed(speed: PlaybackSpeed) {
            _state.value = _state.value.copy(speed = speed)
            player?.setSpeed(speed.factor)
        }

        /** Stops reading: the app went to the background, or the lesson is closing, or the user asked. */
        fun stop() {
            slotSource = null
            playedAny = false
            player?.stop()
            val s = _state.value
            if (s.playingBlockId != null ||
                s.continuous
            ) {
                _state.value = s.copy(playingBlockId = null, continuous = false)
            }
        }

        override fun onCleared() {
            player?.release()
            player = null
        }

        /** Test hook: [onCleared] is protected and a plain unit test has no ViewModelStore to clear. */
        internal fun clearForTest() {
            viewModelScope.coroutineContext[Job]?.cancel()
            onCleared()
        }
    }
