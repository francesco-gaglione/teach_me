package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.Mission
import com.teachme.core.model.Page
import com.teachme.core.model.Preference
import com.teachme.core.model.TopicMission
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.path
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/mission.md` section 4. */
interface MissionApi {
    suspend fun getMission(topicId: String): ApiResult<TopicMission>

    suspend fun listVersions(
        topicId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<Mission>>

    suspend fun confirm(
        topicId: String,
        version: Int,
    ): ApiResult<TopicMission>

    suspend fun listPreferences(
        topicId: String?,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<Preference>>

    suspend fun deletePreference(id: String): ApiResult<Unit>
}

@Singleton
class KtorMissionApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : MissionApi {
        override suspend fun getMission(topicId: String): ApiResult<TopicMission> =
            apiCall({ client.get { url { path("api", "v1", "topics", topicId, "mission") } } }) {
                it.body<TopicMissionDto>().toModel()
            }

        override suspend fun listVersions(
            topicId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Mission>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "topics", topicId, "mission", "versions") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { response -> response.body<PageDto<MissionDto>>().toModel { it.toModel() } }

        override suspend fun confirm(
            topicId: String,
            version: Int,
        ): ApiResult<TopicMission> =
            apiCall({
                client.post {
                    url { path("api", "v1", "topics", topicId, "mission", "confirm") }
                    contentType(ContentType.Application.Json)
                    setBody(ConfirmMissionRequest(version))
                }
            }) { it.body<TopicMissionDto>().toModel() }

        override suspend fun listPreferences(
            topicId: String?,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Preference>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "preferences") }
                    topicId?.let { parameter("topic_id", it) }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { response ->
                val dto = response.body<PageDto<PreferenceDto>>()
                // A preference of a scope this app does not know is skipped.
                Page(dto.items.mapNotNull { it.toModelOrNull() }, dto.nextCursor)
            }

        override suspend fun deletePreference(id: String): ApiResult<Unit> =
            apiCall({ client.delete { url { path("api", "v1", "preferences", id) } } }) { }
    }
