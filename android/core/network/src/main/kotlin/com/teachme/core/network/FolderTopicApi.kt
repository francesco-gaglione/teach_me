package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.Folder
import com.teachme.core.model.Page
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.delete
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.client.request.patch
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.http.path
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/folders-topics.md` §2, one call per endpoint (no pagination loop). */
interface FolderApi {
    suspend fun listFolders(
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<Folder>>

    suspend fun getFolder(id: String): ApiResult<Folder>

    suspend fun createFolder(name: String): ApiResult<Folder>

    suspend fun renameFolder(
        id: String,
        name: String,
    ): ApiResult<Folder>

    suspend fun deleteFolder(id: String): ApiResult<Unit>
}

interface TopicApi {
    suspend fun listTopics(
        folderId: String,
        cursor: String? = null,
        limit: Int? = null,
    ): ApiResult<Page<Topic>>

    suspend fun getTopic(id: String): ApiResult<Topic>

    suspend fun createTopic(
        folderId: String,
        title: String,
        language: TopicLanguage = TopicLanguage.It,
    ): ApiResult<Topic>

    suspend fun updateTopic(
        id: String,
        title: String? = null,
        folderId: String? = null,
    ): ApiResult<Topic>

    suspend fun deleteTopic(id: String): ApiResult<Unit>
}

@Singleton
class KtorFolderApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : FolderApi {
        override suspend fun listFolders(
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Folder>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "folders") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<FolderDto>>().toModel(FolderDto::toModel) }

        override suspend fun getFolder(id: String): ApiResult<Folder> =
            apiCall({ client.get { url { path("api", "v1", "folders", id) } } }) { it.body<FolderDto>().toModel() }

        override suspend fun createFolder(name: String): ApiResult<Folder> =
            apiCall({
                client.post {
                    url { path("api", "v1", "folders") }
                    contentType(ContentType.Application.Json)
                    setBody(CreateFolderRequest(name))
                }
            }) { it.body<FolderDto>().toModel() }

        override suspend fun renameFolder(
            id: String,
            name: String,
        ): ApiResult<Folder> =
            apiCall({
                client.patch {
                    url { path("api", "v1", "folders", id) }
                    contentType(ContentType.Application.Json)
                    setBody(UpdateFolderRequest(name))
                }
            }) { it.body<FolderDto>().toModel() }

        override suspend fun deleteFolder(id: String): ApiResult<Unit> =
            apiCall({ client.delete { url { path("api", "v1", "folders", id) } } }) { }
    }

@Singleton
class KtorTopicApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : TopicApi {
        override suspend fun listTopics(
            folderId: String,
            cursor: String?,
            limit: Int?,
        ): ApiResult<Page<Topic>> =
            apiCall({
                client.get {
                    url { path("api", "v1", "folders", folderId, "topics") }
                    cursor?.let { parameter("cursor", it) }
                    limit?.let { parameter("limit", it) }
                }
            }) { it.body<PageDto<TopicDto>>().toModel(TopicDto::toModel) }

        override suspend fun getTopic(id: String): ApiResult<Topic> =
            apiCall({ client.get { url { path("api", "v1", "topics", id) } } }) { it.body<TopicDto>().toModel() }

        override suspend fun createTopic(
            folderId: String,
            title: String,
            language: TopicLanguage,
        ): ApiResult<Topic> {
            require(language != TopicLanguage.Unknown) { "A Topic cannot be created in an unknown language" }
            return apiCall({
                client.post {
                    url { path("api", "v1", "folders", folderId, "topics") }
                    contentType(ContentType.Application.Json)
                    setBody(CreateTopicRequest(title, language.wire.orEmpty()))
                }
            }) { it.body<TopicDto>().toModel() }
        }

        override suspend fun updateTopic(
            id: String,
            title: String?,
            folderId: String?,
        ): ApiResult<Topic> =
            apiCall({
                client.patch {
                    url { path("api", "v1", "topics", id) }
                    contentType(ContentType.Application.Json)
                    setBody(UpdateTopicRequest(title, folderId))
                }
            }) { it.body<TopicDto>().toModel() }

        override suspend fun deleteTopic(id: String): ApiResult<Unit> =
            apiCall({ client.delete { url { path("api", "v1", "topics", id) } } }) { }
    }
