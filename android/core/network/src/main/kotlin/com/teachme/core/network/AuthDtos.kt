package com.teachme.core.network

import com.teachme.core.model.AuthUser
import com.teachme.core.model.LlmProvider
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** `User` of users.md section 1. Unknown fields are ignored; an unknown `llm_provider` becomes [LlmProvider.Unknown]. */
@Serializable
internal data class UserDto(
    val id: String,
    val username: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("has_llm_key") val hasLlmKey: Boolean,
    @SerialName("llm_provider") val llmProvider: String? = null,
    @SerialName("tts_voice") val ttsVoice: String? = null,
) {
    fun toModel() = AuthUser(id, username, displayName, hasLlmKey, LlmProvider.fromApi(llmProvider), ttsVoice)
}

/** `Session` of `POST /auth/login`. The token is a secret: [toString] hides it. */
@Serializable
internal class SessionDto(
    val token: String,
    val user: UserDto,
) {
    override fun toString(): String = "SessionDto(user=${user.id})"
}

/** The password never appears in [toString]. */
@Serializable
internal class LoginRequest(
    val username: String,
    val password: String,
    @SerialName("device_name") val deviceName: String? = null,
) {
    override fun toString(): String = "LoginRequest(username=$username)"
}

@Serializable
internal class ChangePasswordRequest(
    @SerialName("current_password") val currentPassword: String,
    @SerialName("new_password") val newPassword: String,
) {
    override fun toString(): String = "ChangePasswordRequest()"
}

@Serializable
internal class SetLlmKeyRequest(
    @SerialName("api_key") val apiKey: String,
) {
    override fun toString(): String = "SetLlmKeyRequest()"
}

@Serializable
internal class SetTtsVoiceRequest(
    val voice: String,
)

/** `GET /health` (liveness, no authentication). */
@Serializable
internal data class LivenessDto(
    val status: String,
)
