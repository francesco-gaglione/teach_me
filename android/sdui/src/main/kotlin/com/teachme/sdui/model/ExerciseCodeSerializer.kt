package com.teachme.sdui.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

private const val DEFAULT_LANGUAGE = "text"

/**
 * Reads the optional `code` of an exercise (sdui.md section 4.1) and never fails: a value that is not an object, or
 * whose `code` is missing, not a string or blank, is dropped (null) so the exercise still draws without it
 * (tolerance of sdui.md section 1). A missing or wrongly typed `language` falls back to plain text.
 */
internal object ExerciseCodeSerializer : KSerializer<ExerciseCode?> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("ExerciseCode")

    override fun deserialize(decoder: Decoder): ExerciseCode? {
        val element = (decoder as? JsonDecoder ?: throw SerializationException("JSON only")).decodeJsonElement()
        val obj = element as? JsonObject ?: return null
        val code = (obj["code"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotBlank() }
        val language =
            (obj["language"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotBlank() }
        return code?.let { ExerciseCode(language ?: DEFAULT_LANGUAGE, it) }
    }

    override fun serialize(
        encoder: Encoder,
        value: ExerciseCode?,
    ): Unit = throw SerializationException("Blocks are never written back to JSON by the app")
}
