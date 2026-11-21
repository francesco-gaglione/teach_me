package com.teachme.core.data

import okhttp3.Cache
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import java.io.File

private const val CLIP_CACHE_BYTES = 100L * 1024 * 1024
private const val API_PREFIX = "api/v1"

/**
 * HTTP side of the clip player, kept apart from ExoPlayer so it can be tested: the OkHttp client with its bounded disk
 * cache (clips are served `private, immutable`, so a replay never downloads again) and the URL rules.
 */
internal object ClipHttp {
    /**
     * Client for clip requests. Redirects are never followed: the `Authorization` header must not leave the configured
     * host. One instance per process, because one [Cache] directory cannot be shared by two caches.
     */
    fun client(
        cacheDir: File,
        maxBytes: Long = CLIP_CACHE_BYTES,
    ): OkHttpClient =
        OkHttpClient
            .Builder()
            .cache(Cache(cacheDir, maxBytes))
            .followRedirects(false)
            .followSslRedirects(false)
            .build()

    /**
     * Resolves [clipPath] (relative to `/api/v1`, with or without a leading slash) against the backend address
     * [baseUrl]. Returns null for an unusable base, or when the result would leave the configured host, scheme, port or
     * the `/api/v1` tree (an absolute URL, `//other-host/...`, `..` segments).
     */
    fun resolve(
        baseUrl: String,
        clipPath: String,
    ): HttpUrl? {
        val base = baseUrl.trimEnd('/').plus("/$API_PREFIX/").toHttpUrlOrNull() ?: return null
        val relative = clipPath.trimStart('/')
        // An absolute URL ("http://other/x") would replace the base: refuse it before resolving.
        if (relative.contains("://")) return null
        val url = base.resolve(relative) ?: return null
        val sameOrigin = url.scheme == base.scheme && url.host == base.host && url.port == base.port
        return url.takeIf { sameOrigin && url.encodedPath.startsWith(base.encodedPath) }
    }

    fun authorization(token: String): Map<String, String> = mapOf("Authorization" to "Bearer ${token.trim()}")
}

/**
 * Tells which playback events belong to the clip being played. Every [play] and [stop] opens a new generation; the
 * media id of the item carries it, so an event of an older clip delivered late is recognised and dropped.
 */
internal class PlaybackGeneration {
    private var current = 0

    /** Opens a new generation and returns the media id to give to its item. */
    fun next(): String {
        current++
        return current.toString()
    }

    /** Invalidates whatever is playing (stop). */
    fun invalidate() {
        current++
    }

    /** True when an event seen on an item with [mediaId] belongs to the clip being played now. */
    fun isCurrent(mediaId: String?): Boolean = mediaId == current.toString()
}
