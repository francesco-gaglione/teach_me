package com.teachme.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Error envelope of api-conventions §7: `{"error": {...}}`. */
@Serializable
data class ApiErrorResponse(
    val error: ApiError,
)

@Serializable
data class ApiError(
    val code: String,
    val message: String = "",
    val details: List<ApiErrorDetail>? = null,
    @SerialName("request_id") val requestId: String? = null,
)

@Serializable
data class ApiErrorDetail(
    val field: String? = null,
    val issue: String? = null,
)
