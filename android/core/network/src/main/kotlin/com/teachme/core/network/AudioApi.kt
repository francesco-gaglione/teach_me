package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioClip
import com.teachme.core.model.AudioManifest
import com.teachme.core.model.AudioStatus
import com.teachme.core.model.BlockAudio
import com.teachme.core.model.TtsVoice
import com.teachme.core.model.TtsVoices
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.http.path
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import javax.inject.Inject
import javax.inject.Singleton

@Serializable
internal data class AudioClipDto(
    val url: String,
    val voice: String = "",
    @SerialName("duration_ms") val durationMs: Long = 0,
)

@Serializable
internal data class BlockAudioDto(
    @SerialName("block_id") val blockId: String,
    val status: String? = null,
    val clip: AudioClipDto? = null,
)

@Serializable
internal data class AudioManifestDto(
    val voice: String? = null,
    val blocks: List<BlockAudioDto> = emptyList(),
)

@Serializable
internal data class TtsVoiceDto(
    val id: String,
    val label: String = "",
    @SerialName("sample_url") val sampleUrl: String? = null,
)

@Serializable
internal data class TtsVoicesDto(
    val available: Boolean = false,
    @SerialName("default_voice") val defaultVoice: String? = null,
    val voices: List<TtsVoiceDto> = emptyList(),
)

internal fun AudioManifestDto.toModel() =
    AudioManifest(
        voice,
        blocks.map { b ->
            BlockAudio(
                b.blockId,
                AudioStatus.fromWire(b.status),
                b.clip?.let { AudioClip(it.url, it.voice, it.durationMs) },
            )
        },
    )

internal fun TtsVoicesDto.toModel() =
    TtsVoices(available, defaultVoice, voices.map { TtsVoice(it.id, it.label, it.sampleUrl) })

/** Raw endpoints of `docs/contracts/audio.md` sections 3 and 4. */
interface AudioApi {
    suspend fun getManifest(lessonId: String): ApiResult<AudioManifest>

    suspend fun getVoices(): ApiResult<TtsVoices>
}

@Singleton
class KtorAudioApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : AudioApi {
        override suspend fun getManifest(lessonId: String): ApiResult<AudioManifest> =
            apiCall({ client.get { url { path("api", "v1", "lessons", lessonId, "audio") } } }) {
                it.body<AudioManifestDto>().toModel()
            }

        override suspend fun getVoices(): ApiResult<TtsVoices> =
            apiCall({ client.get { url { path("api", "v1", "tts", "voices") } } }) {
                it.body<TtsVoicesDto>().toModel()
            }
    }
