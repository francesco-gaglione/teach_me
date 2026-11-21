package com.teachme.feature.settings

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** AboutLibraries export (`licenses.json` generated at build time): libraries plus a license map keyed by id. */
@Serializable
internal data class LicenseText(
    val name: String = "",
    val url: String? = null,
    val content: String? = null,
)

@Serializable
internal data class LicenseDeveloper(
    val name: String? = null,
)

@Serializable
internal data class LicenseLibrary(
    val uniqueId: String = "",
    val name: String? = null,
    val artifactVersion: String? = null,
    val website: String? = null,
    val developers: List<LicenseDeveloper> = emptyList(),
    val licenses: List<String> = emptyList(),
)

@Serializable
internal data class LicenseFile(
    val licenses: Map<String, LicenseText> = emptyMap(),
    val libraries: List<LicenseLibrary> = emptyList(),
)

/** One row of the licenses screen: the library plus the full text of its license. */
internal data class LicenseEntry(
    val name: String,
    val version: String,
    val licenseName: String,
    val copyright: String,
    val text: String,
    /** Link to show when the license has no bundled text (or the library website). */
    val url: String = "",
)

internal const val LICENSES_ASSET = "licenses.json"

private val licensesJson = Json { ignoreUnknownKeys = true }

/** Parses the generated `licenses.json`; null when it is missing or malformed ("not available in this build"). */
internal fun parseLicenses(raw: String?): List<LicenseEntry>? {
    if (raw == null) return null
    val file = runCatching { licensesJson.decodeFromString<LicenseFile>(raw) }.getOrNull() ?: return null
    if (file.libraries.isEmpty()) return null
    return file.libraries
        .distinctBy { it.uniqueId }
        .map { lib ->
            val licenses = lib.licenses.map { id -> id to file.licenses[id] }
            val names = licenses.map { (id, l) -> l?.name?.takeIf { it.isNotBlank() } ?: id }
            val text = licenses.mapNotNull { (_, l) -> l?.content?.takeIf { it.isNotBlank() } }.joinToString("\n\n")
            val licenseUrl = licenses.firstNotNullOfOrNull { (_, l) -> l?.url?.takeIf { it.isNotBlank() } }
            LicenseEntry(
                name = lib.name?.takeIf { it.isNotBlank() } ?: lib.uniqueId,
                version = lib.artifactVersion.orEmpty(),
                licenseName = names.joinToString(", ").ifBlank { "?" },
                copyright = lib.developers.mapNotNull { it.name?.takeIf(String::isNotBlank) }.joinToString(", "),
                text = text,
                url = if (text.isBlank()) licenseUrl ?: lib.website.orEmpty() else lib.website.orEmpty(),
            )
        }.sortedBy { it.name.lowercase() }
}
