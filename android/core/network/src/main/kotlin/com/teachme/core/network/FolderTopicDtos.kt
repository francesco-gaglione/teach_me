package com.teachme.core.network

import com.teachme.core.model.Folder
import com.teachme.core.model.Page
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicState
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime

@Serializable
internal data class FolderDto(
    val id: String,
    val name: String,
    val position: Int,
    @SerialName("topic_count") val topicCount: Int,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
)

@Serializable
internal data class TopicDto(
    val id: String,
    @SerialName("folder_id") val folderId: String,
    val title: String,
    val state: String? = null,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
    val language: String? = null,
)

@Serializable
internal data class PageDto<T>(
    val items: List<T>,
    @SerialName("next_cursor") val nextCursor: String? = null,
)

@Serializable
internal data class CreateFolderRequest(
    val name: String,
)

@Serializable
internal data class UpdateFolderRequest(
    val name: String? = null,
)

@Serializable
internal data class CreateTopicRequest(
    val title: String,
    val language: String,
)

@Serializable
internal data class UpdateTopicRequest(
    val title: String? = null,
    @SerialName("folder_id") val folderId: String? = null,
)

/** RFC 3339 with any offset (api-conventions §5). */
private fun parseInstant(value: String): Instant = OffsetDateTime.parse(value).toInstant()

internal fun FolderDto.toModel() =
    Folder(id, name, position, topicCount, parseInstant(createdAt), parseInstant(updatedAt))

internal fun TopicDto.toModel() =
    Topic(
        id,
        folderId,
        title,
        TopicState.fromWire(state),
        parseInstant(createdAt),
        parseInstant(updatedAt),
        TopicLanguage.fromWire(language),
    )

internal fun <D, M> PageDto<D>.toModel(transform: (D) -> M) = Page(items.map(transform), nextCursor)
