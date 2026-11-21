package com.teachme.sdui.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class GlossaryEntry(
    val key: String,
    val term: String,
    val definition: String,
    val avoid: List<String> = emptyList(),
)

@Serializable
data class SourceEntry(
    val key: String,
    @SerialName("resource_id") val resourceId: String? = null,
    val title: String,
    /**
     * `https` URL, or an explicit JSON `null` for a PDF the learner uploaded (sdui.md section 2, v1.1): such a source
     * has no link and the pages are already part of [title]. A missing `url` field is malformed: [LessonParser] drops it.
     */
    val url: String? = null,
) {
    /** A source taken from the learner's own PDF (no URL): shown as "Dal tuo PDF", never as a link. */
    val isUserPdf: Boolean get() = url == null
}

/** The newest catalog this build knows. A document with a higher `catalog_version` is still rendered. */
const val SUPPORTED_CATALOG_VERSION = 1

/** A lesson as the renderer sees it: tolerant, already normalized (see [LessonParser]). */
data class LessonDocument(
    val catalogVersion: Int,
    val title: String,
    val objective: String,
    val estimatedMinutes: Int,
    val glossary: List<GlossaryEntry>,
    val sources: List<SourceEntry>,
    val blocks: List<Block>,
) {
    val newerCatalog: Boolean get() = catalogVersion > SUPPORTED_CATALOG_VERSION

    /** The first block when it is a level 1 heading: the lesson title. Other level 1 headings are plain headings. */
    val titleBlockId: String? get() = (blocks.firstOrNull() as? Heading)?.takeIf { it.level == 1 }?.id

    fun glossaryEntry(key: String): GlossaryEntry? = glossary.firstOrNull { it.key == key }

    fun source(key: String): SourceEntry? = sources.firstOrNull { it.key == key }

    /** 1-based position of the source in `sources[]`, the number shown in citations; null if unknown. */
    fun sourceNumber(key: String): Int? = sources.indexOfFirst { it.key == key }.takeIf { it >= 0 }?.plus(1)
}

/** Something worth recording about a document (logged by the host; never shown to the learner). */
sealed interface SduiEvent {
    data class UnknownBlockType(
        val id: String,
        val type: String?,
    ) : SduiEvent

    data class MalformedBlock(
        val id: String,
        val type: String?,
        val detail: String?,
    ) : SduiEvent

    data class DuplicateBlockId(
        val id: String,
    ) : SduiEvent

    data class WantedComponent(
        val wanted: String,
        val purpose: String,
    ) : SduiEvent

    data class NewerCatalog(
        val version: Int,
    ) : SduiEvent
}

sealed interface ParseResult {
    data class Usable(
        val document: LessonDocument,
        val events: List<SduiEvent>,
    ) : ParseResult

    /** Not JSON, not an object, no `blocks`, or no block can be drawn: "Questa lezione non si può aprire". */
    data class Unusable(
        val reason: String,
    ) : ParseResult
}
