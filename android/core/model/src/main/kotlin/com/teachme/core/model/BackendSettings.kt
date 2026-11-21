package com.teachme.core.model

import kotlinx.coroutines.flow.Flow

/**
 * What the HTTP client needs for a request: the backend address, the session token ([token] is blank when nobody is
 * signed in) and the user the token belongs to ([userId], blank without a session). The token is a secret: it is
 * kept out of [toString].
 */
data class BackendSettings(
    val baseUrl: String = "",
    val token: String = "",
    val userId: String = "",
) {
    /** True when the app has an address to talk to. */
    val isConfigured: Boolean get() = baseUrl.isNotBlank()

    override fun toString(): String =
        "BackendSettings(baseUrl=$baseUrl, userId=$userId, hasToken=${token.isNotBlank()})"
}

/**
 * Cleans a user-typed base URL: trims spaces, lowercases the scheme (`HTTP://` becomes `http://`),
 * removes trailing slashes and a trailing `/api/v1` (the client adds the prefix itself).
 * Single source of truth: both the Settings validation and the HTTP client use it.
 */
fun String.normalizedBaseUrl(): String {
    var url = trim().trimEnd('/')
    val schemeEnd = url.indexOf("://")
    if (schemeEnd > 0) url = url.substring(0, schemeEnd).lowercase() + url.substring(schemeEnd)
    if (url.endsWith(API_PREFIX)) url = url.removeSuffix(API_PREFIX).trimEnd('/')
    return url
}

private const val API_PREFIX = "/api/v1"

/** Persistent storage of [BackendSettings] (address and session token). Implemented in `:core:data`. */
interface SettingsRepository {
    /** Emits the stored settings and every later change. */
    val settings: Flow<BackendSettings>

    suspend fun current(): BackendSettings

    suspend fun update(settings: BackendSettings)
}
