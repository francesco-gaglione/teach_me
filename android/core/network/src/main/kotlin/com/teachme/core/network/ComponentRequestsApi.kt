package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ComponentRequestExample
import com.teachme.core.model.ComponentRequestGroup
import com.teachme.core.model.Page
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.http.path
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.time.Instant
import java.time.OffsetDateTime
import javax.inject.Inject
import javax.inject.Singleton

/** `GET /component-requests` (`docs/contracts/component-requests.md`). */
interface ComponentRequestsApi {
    suspend fun list(cursor: String?): ApiResult<Page<ComponentRequestGroup>>
}

@Serializable
internal data class ComponentRequestExampleDto(
    val purpose: String = "",
    @SerialName("lesson_title") val lessonTitle: String = "",
    @SerialName("topic_title") val topicTitle: String = "",
    @SerialName("requested_at") val requestedAt: String? = null,
)

@Serializable
internal data class ComponentRequestGroupDto(
    val wanted: String,
    val count: Int = 0,
    @SerialName("first_requested_at") val firstRequestedAt: String? = null,
    @SerialName("last_requested_at") val lastRequestedAt: String? = null,
    @SerialName("catalog_version") val catalogVersion: Int = 1,
    val examples: List<ComponentRequestExampleDto> = emptyList(),
)

private fun parseInstant(value: String?): Instant? =
    value?.let { runCatching { OffsetDateTime.parse(it).toInstant() }.getOrNull() }

internal fun ComponentRequestGroupDto.toModel() =
    ComponentRequestGroup(
        wanted = wanted,
        count = count,
        firstRequestedAt = parseInstant(firstRequestedAt),
        lastRequestedAt = parseInstant(lastRequestedAt),
        catalogVersion = catalogVersion,
        examples =
            examples.map {
                ComponentRequestExample(it.purpose, it.lessonTitle, it.topicTitle, parseInstant(it.requestedAt))
            },
    )

@Singleton
class KtorComponentRequestsApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : ComponentRequestsApi {
        override suspend fun list(cursor: String?): ApiResult<Page<ComponentRequestGroup>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "component-requests") }
                    cursor?.let { parameter("cursor", it) }
                }
            }) { it.body<PageDto<ComponentRequestGroupDto>>().toModel { dto -> dto.toModel() } }
    }
