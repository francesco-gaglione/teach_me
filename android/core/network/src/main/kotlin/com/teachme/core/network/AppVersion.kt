package com.teachme.core.network

/** App version sent in the `X-App-Version` header. Provided by `:app`. */
data class AppVersion(
    val value: String,
)
