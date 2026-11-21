package com.teachme.core.network

import kotlinx.serialization.json.Json

/** Shared JSON configuration: unknown fields are ignored (api-conventions §1). */
val ApiJson: Json =
    Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        coerceInputValues = true
    }
