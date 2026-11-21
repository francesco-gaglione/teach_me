package com.teachme.sdui.model

import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull

/** Documents above this size are refused (a real lesson is a few tens of KiB). */
const val MAX_DOCUMENT_CHARS = 512 * 1024

/**
 * Parses a lesson document (docs/contracts/sdui.md) tolerantly: it never throws. A block of unknown type
 * or one that does not fit becomes [Unavailable]; the rest of the lesson is intact. Only a document that
 * is not JSON, has no `blocks`, or has nothing drawable is [ParseResult.Unusable].
 */
object LessonParser {
    internal val json = Json { ignoreUnknownKeys = true }

    fun parse(text: String): ParseResult {
        if (text.length > MAX_DOCUMENT_CHARS) return ParseResult.Unusable("document too large")
        val root =
            try {
                json.parseToJsonElement(text)
            } catch (e: SerializationException) {
                return ParseResult.Unusable("not JSON (${e::class.simpleName})")
            } catch (e: IllegalArgumentException) {
                return ParseResult.Unusable("not JSON (${e::class.simpleName})")
            }
        val obj = root as? JsonObject ?: return ParseResult.Unusable("the document is not an object")
        val blocksJson = obj["blocks"] as? JsonArray ?: return ParseResult.Unusable("no blocks")
        val events = mutableListOf<SduiEvent>()

        val glossary = decodeEntries(obj["glossary"], GlossaryEntry.serializer()).distinctBy { it.key }
        val sources = decodeEntries(obj["sources"], SourceEntry.serializer(), requiredKey = "url").distinctBy { it.key }
        val catalogVersion = (obj["catalog_version"] as? JsonPrimitive)?.intOrNull ?: 1
        if (catalogVersion > SUPPORTED_CATALOG_VERSION) events += SduiEvent.NewerCatalog(catalogVersion)

        val decoded = blocksJson.map { decodeBlock(json, it, inFallback = false) }
        val normalizer = Normalizer(glossary.map { it.key }.toSet(), sources.map { it.key }.toSet(), events)
        val blocks = normalizer.normalize(decoded)
        if (blocks.none { it !is Unavailable }) return ParseResult.Unusable("no drawable block")

        val document =
            LessonDocument(
                catalogVersion = catalogVersion,
                title = obj.string("title").orEmpty(),
                objective = obj.string("objective").orEmpty(),
                estimatedMinutes = (obj["estimated_minutes"] as? JsonPrimitive)?.intOrNull ?: 0,
                glossary = glossary,
                sources = sources,
                blocks = blocks,
            )
        return ParseResult.Usable(document, events)
    }

    private fun <T> decodeEntries(
        element: JsonElement?,
        serializer: kotlinx.serialization.KSerializer<T>,
        /** An entry without this key is malformed and dropped, even if the field is nullable (`url`: null is explicit). */
        requiredKey: String? = null,
    ): List<T> =
        (element as? JsonArray).orEmpty().mapNotNull {
            if (requiredKey != null && (it as? JsonObject)?.containsKey(requiredKey) != true) return@mapNotNull null
            try {
                json.decodeFromJsonElement(serializer, it)
            } catch (e: SerializationException) {
                null
            } catch (e: IllegalArgumentException) {
                null
            }
        }

    private fun JsonObject.string(key: String): String? =
        (this[key] as? JsonPrimitive)
            ?.takeIf {
                it.isString
            }?.contentOrNull
}

private const val MIN_ANSWER_CHARS = 50
private const val MAX_ANSWER_CHARS = 2000

/** Second pass: duplicate ids (first wins), renderability, and the events worth logging. */
private class Normalizer(
    private val glossaryKeys: Set<String>,
    private val sourceKeys: Set<String>,
    private val events: MutableList<SduiEvent>,
) {
    private val seenIds = mutableSetOf<String>()

    fun normalize(blocks: List<Block>): List<Block> = blocks.mapNotNull { normalizeOne(it) }

    private fun normalizeOne(block: Block): Block? {
        if (block.id.isNotEmpty() && !seenIds.add(block.id)) {
            events += SduiEvent.DuplicateBlockId(block.id)
            return null
        }
        val problem = renderabilityProblem(block, glossaryKeys, sourceKeys)
        val result: Block =
            when {
                problem != null -> Unavailable(block.id, block.type, UnavailableReason.Invalid, problem)
                block is MissingComponent -> normalizeMissing(block)
                block is Diagram -> block.also { recordIfNotDrawable(it) }
                block is OpenAnswer ->
                    block.copy(
                        maxChars = block.maxChars.coerceIn(MIN_ANSWER_CHARS, MAX_ANSWER_CHARS),
                    )
                else -> block
            }
        if (result is Unavailable) record(result)
        return result
    }

    private fun normalizeMissing(block: MissingComponent): Block {
        events += SduiEvent.WantedComponent(block.wanted, block.purpose)
        val fallback = block.fallback.mapNotNull { normalizeOne(it) }
        return if (fallback.any { it !is Unavailable }) {
            block.copy(fallback = fallback)
        } else {
            Unavailable(block.id, block.type, UnavailableReason.NoFallback, "no drawable fallback block")
        }
    }

    /** A diagram the app falls back to text for is worth a log line, although the learner still gets its `alt`. */
    private fun recordIfNotDrawable(block: Diagram) {
        val result = validateDiagram(block)
        if (result is DiagramValidation.TextOnly) {
            events += SduiEvent.MalformedBlock(block.id, block.type, "diagram: ${result.problem}")
        }
    }

    private fun record(block: Unavailable) {
        events +=
            when (block.reason) {
                UnavailableReason.UnknownType -> SduiEvent.UnknownBlockType(block.id, block.rawType)
                else -> SduiEvent.MalformedBlock(block.id, block.rawType, block.detail)
            }
    }
}
