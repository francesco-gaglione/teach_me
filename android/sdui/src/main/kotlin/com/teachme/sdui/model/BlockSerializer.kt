package com.teachme.sdui.model

import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/** Registry of the leaf block serializers by wire `type`: the polymorphic part of the model. */
private val LEAF_SERIALIZERS: Map<String, DeserializationStrategy<Block>> =
    mapOf(
        "heading" to Heading.serializer(),
        "paragraph" to Paragraph.serializer(),
        "callout" to Callout.serializer(),
        "code" to CodeBlock.serializer(),
        "glossary_term" to GlossaryTermBlock.serializer(),
        "quiz_single" to QuizSingle.serializer(),
        "quiz_multi" to QuizMulti.serializer(),
        "fill_blank" to FillBlank.serializer(),
        "order_steps" to OrderSteps.serializer(),
        "flashcard" to Flashcard.serializer(),
        "open_answer" to OpenAnswer.serializer(),
        "checklist" to Checklist.serializer(),
        "primary_source" to PrimarySource.serializer(),
        "lesson_link" to LessonLink.serializer(),
        "ask_agent" to AskAgent.serializer(),
        "diagram" to Diagram.serializer(),
    )

/** Wire names of the catalog v1, `missing_component` included. */
val CATALOG_V1_TYPES: Set<String> = LEAF_SERIALIZERS.keys + "missing_component"

private fun JsonObject.string(key: String): String? =
    (this[key] as? JsonPrimitive)
        ?.takeIf {
            it.isString
        }?.contentOrNull

/**
 * Turns one JSON element into a [Block]. It never throws: an unknown `type` or a block that does not fit
 * its type becomes [Unavailable]. [inFallback] is true inside a `missing_component.fallback`, where another
 * `missing_component` is not expanded.
 */
internal fun decodeBlock(
    json: Json,
    element: JsonElement,
    inFallback: Boolean,
): Block {
    val obj = element as? JsonObject
    val type = obj?.string("type")
    val id = obj?.string("id").orEmpty()
    return when {
        obj == null -> Unavailable(id, null, UnavailableReason.Malformed, "block is not an object")
        type == null -> Unavailable(id, null, UnavailableReason.Malformed, "missing type")
        type == "missing_component" ->
            if (inFallback) {
                Unavailable(id, type, UnavailableReason.NoFallback, "nested missing_component")
            } else {
                decodeMissingComponent(json, obj, id)
            }
        else -> {
            val strategy = LEAF_SERIALIZERS[type]
            if (strategy == null) {
                Unavailable(id, type, UnavailableReason.UnknownType)
            } else {
                try {
                    json.decodeFromJsonElement(strategy, obj)
                } catch (e: SerializationException) {
                    Unavailable(id, type, UnavailableReason.Malformed, e::class.simpleName)
                } catch (e: IllegalArgumentException) {
                    Unavailable(id, type, UnavailableReason.Malformed, e::class.simpleName)
                }
            }
        }
    }
}

private fun decodeMissingComponent(
    json: Json,
    obj: JsonObject,
    id: String,
): Block {
    val fallbackElements = obj["fallback"] as? JsonArray
    if (id.isEmpty() || fallbackElements == null) {
        return Unavailable(id, "missing_component", UnavailableReason.NoFallback, "no fallback")
    }
    val fallback = fallbackElements.map { decodeBlock(json, it, inFallback = true) }
    return MissingComponent(
        id = id,
        lang = obj.string("lang"),
        wanted = obj.string("wanted").orEmpty(),
        purpose = obj.string("purpose").orEmpty(),
        fallback = fallback,
    )
}

/**
 * Polymorphic [KSerializer] for a single [Block] on top of kotlinx.serialization (decode only). Use it with
 * a [Json] that has `ignoreUnknownKeys = true`; it never throws on unknown or malformed blocks.
 */
object BlockSerializer : KSerializer<Block> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("Block")

    override fun deserialize(decoder: Decoder): Block {
        val jsonDecoder = decoder as? JsonDecoder ?: throw SerializationException("Blocks are JSON only")
        return decodeBlock(jsonDecoder.json, jsonDecoder.decodeJsonElement(), inFallback = false)
    }

    override fun serialize(
        encoder: Encoder,
        value: Block,
    ): Unit = throw SerializationException("Blocks are never written back to JSON by the app")
}
