package com.teachme.core.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonPrimitive

/** Body of `GET /api/v1/health` (api-conventions §3). */
@Serializable
data class HealthResponse(
    val status: String,
    val version: String,
    @SerialName("api_version") @Serializable(with = ApiVersionSerializer::class) val apiVersion: Int = 0,
    @SerialName("catalog_version") val catalogVersion: Int,
    @SerialName("server_time") val serverTime: String,
)

/**
 * Reads `api_version` tolerantly: an integer, or a string whose integer part is the major ("1.2" is 1). A missing,
 * null or unreadable value is 0, which counts as compatible: an odd answer must never lock the app.
 */
object ApiVersionSerializer : KSerializer<Int> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor("ApiVersion", PrimitiveKind.INT)

    override fun deserialize(decoder: Decoder): Int {
        val element = (decoder as? JsonDecoder)?.decodeJsonElement() as? JsonPrimitive ?: return 0
        return parseApiMajor(element.content)
    }

    override fun serialize(
        encoder: Encoder,
        value: Int,
    ) = encoder.encodeInt(value)
}

/** The integer part of [raw] ("1", "1.2", " 2.0.1 "), or 0 when there is none. */
internal fun parseApiMajor(raw: String): Int =
    raw
        .trim()
        .substringBefore('.')
        .toIntOrNull()
        ?.takeIf { it >= 0 } ?: 0

/** The major of `api_version` this app build speaks (api-conventions section 1, `/api/v1`). Raise it with a new `/api/vN` client. */
const val SUPPORTED_API_VERSION = 1

/**
 * True when the server's API major is higher than the one this app supports: the app is too old and must be updated
 * (api-conventions section 1, "Compatibilità e supporto"). The same major or a lower one is fine: changes inside a
 * major are only additive.
 */
fun HealthResponse.requiresAppUpdate(supported: Int = SUPPORTED_API_VERSION): Boolean = apiVersion > supported

/** Outcome of the connection test, the four states of api-conventions §3. */
sealed interface ConnectionResult {
    data class Connected(
        val health: HealthResponse,
    ) : ConnectionResult

    /** HTTP 401: the session ended. The global handler takes the user to the login; the test shows nothing. */
    data object SessionEnded : ConnectionResult

    /** Timeout, DNS failure, connection refused, TLS error, malformed or missing URL. */
    data class Unreachable(
        val reason: UnreachableReason = UnreachableReason.Other,
    ) : ConnectionResult

    /** Anything else; [httpStatus] is null if no status was received. */
    data class Unexpected(
        val httpStatus: Int?,
        val error: ApiError? = null,
    ) : ConnectionResult
}

/** Why the backend could not be reached; lets the UI say more than "unreachable". */
enum class UnreachableReason { Timeout, UnknownHost, ConnectionRefused, Tls, Other }
