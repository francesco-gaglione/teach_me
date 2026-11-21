package com.teachme.core.model

import kotlinx.coroutines.flow.Flow

/** `status` of a block in the audio manifest (audio.md section 3). A value this app does not know is [Pending]. */
enum class AudioStatus(
    val wire: String,
) {
    Ready("ready"),
    Pending("pending"),
    Failed("failed"),
    ;

    companion object {
        fun fromWire(value: String?): AudioStatus = entries.firstOrNull { it.wire == value } ?: Pending
    }
}

/** A clip to play: [url] is relative to `/api/v1`, [voice] may differ from the user's current one. */
data class AudioClip(
    val url: String,
    val voice: String,
    val durationMs: Long,
)

data class BlockAudio(
    val blockId: String,
    val status: AudioStatus,
    val clip: AudioClip?,
)

/** `GET /lessons/{id}/audio`. With the feature off, [blocks] is empty. */
data class AudioManifest(
    val voice: String?,
    val blocks: List<BlockAudio>,
) {
    /** True while the lesson is waiting for clips: the app asks again (audio.md section 3). */
    val hasPending: Boolean get() = blocks.any { it.status == AudioStatus.Pending }
}

data class TtsVoice(
    val id: String,
    val label: String,
    val sampleUrl: String?,
)

/** `GET /tts/voices`. [available] false means the feature is off: the app hides every listening control. */
data class TtsVoices(
    val available: Boolean,
    val defaultVoice: String?,
    val voices: List<TtsVoice>,
)

/** Audio of lessons (docs/contracts/audio.md). No cache: every call goes out. */
interface AudioRepository {
    suspend fun getManifest(lessonId: String): ApiResult<AudioManifest>

    suspend fun getVoices(): ApiResult<TtsVoices>
}

/** Why a clip stopped on its own. */
enum class PlaybackEnd { Completed, Error }

/**
 * Plays one clip at a time. [play] replaces whatever is playing; [onEnded] is called when the clip reaches its end or
 * fails, never after [stop] or [release]. Implementations are used from the main thread.
 */
interface ClipPlayer {
    fun play(
        clipUrl: String,
        onEnded: (PlaybackEnd) -> Unit,
    )

    /** Playback rate, pitch unchanged. Applies at once to the clip in progress and to every later one. */
    fun setSpeed(factor: Float)

    fun stop()

    fun release()
}

/** Reading speed (audio.md section 6): four steps, pitch unchanged. */
enum class PlaybackSpeed(
    val factor: Float,
    val wire: String,
) {
    X075(0.75f, "0.75"),
    X1(1f, "1"),
    X125(1.25f, "1.25"),
    X15(1.5f, "1.5"),
    ;

    /** The speed after this one when the chip is tapped; the last wraps to the first. */
    fun next(): PlaybackSpeed = entries[(ordinal + 1) % entries.size]

    companion object {
        val Default = X1

        /** An unknown or missing stored value is the default. */
        fun fromWire(value: String?): PlaybackSpeed = entries.firstOrNull { it.wire == value } ?: Default
    }
}

/** Listening preferences that belong to the device, not to the account. */
interface AudioPreferences {
    val speed: Flow<PlaybackSpeed>

    suspend fun setSpeed(speed: PlaybackSpeed)
}

/** Creates the player of one lesson screen. */
fun interface ClipPlayerFactory {
    fun create(): ClipPlayer
}
