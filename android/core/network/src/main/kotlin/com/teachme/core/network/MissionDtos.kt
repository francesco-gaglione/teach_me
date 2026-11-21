package com.teachme.core.network

import com.teachme.core.model.Mission
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.Preference
import com.teachme.core.model.PreferenceScope
import com.teachme.core.model.TopicMission
import com.teachme.core.model.asStartingPoint
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime

@Serializable
internal data class MissionDto(
    @SerialName("topic_id") val topicId: String,
    val version: Int,
    val status: String? = null,
    val why: String,
    // Absent in snapshots saved before contract v1.2: the same as null.
    @SerialName("starting_point") val startingPoint: String? = null,
    @SerialName("success_criteria") val successCriteria: List<String> = emptyList(),
    val constraints: List<String> = emptyList(),
    @SerialName("out_of_scope") val outOfScope: List<String> = emptyList(),
    @SerialName("created_at") val createdAt: String,
    @SerialName("confirmed_at") val confirmedAt: String? = null,
    @SerialName("superseded_at") val supersededAt: String? = null,
)

@Serializable
internal data class TopicMissionDto(
    val current: MissionDto? = null,
    val draft: MissionDto? = null,
)

@Serializable
internal data class PreferenceDto(
    val id: String,
    val scope: String,
    @SerialName("topic_id") val topicId: String? = null,
    val text: String,
    @SerialName("created_at") val createdAt: String,
)

@Serializable
internal data class ConfirmMissionRequest(
    val version: Int,
)

private fun parseInstant(value: String): Instant = OffsetDateTime.parse(value).toInstant()

/** An unknown status is treated as superseded: shown read-only, never offered for confirmation. */
internal fun MissionDto.toModel() =
    Mission(
        topicId = topicId,
        version = version,
        status =
            when (status) {
                "draft" -> MissionStatus.Draft
                "confirmed" -> MissionStatus.Confirmed
                else -> MissionStatus.Superseded
            },
        why = why,
        startingPoint = startingPoint.asStartingPoint(),
        successCriteria = successCriteria,
        constraints = constraints,
        outOfScope = outOfScope,
        createdAt = parseInstant(createdAt),
        confirmedAt = confirmedAt?.let(::parseInstant),
        supersededAt = supersededAt?.let(::parseInstant),
    )

internal fun TopicMissionDto.toModel() = TopicMission(current?.toModel(), draft?.toModel())

/** Null for a scope this app does not know: such a preference is skipped, never a crash. */
internal fun PreferenceDto.toModelOrNull(): Preference? {
    val scopeModel =
        when (scope) {
            "global" -> PreferenceScope.Global
            "topic" -> PreferenceScope.Topic
            else -> return null
        }
    return Preference(id, scopeModel, topicId, text, parseInstant(createdAt))
}
