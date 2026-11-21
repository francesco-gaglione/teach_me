package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatRepository
import com.teachme.core.model.Folder
import com.teachme.core.model.FolderRepository
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.TopicRepository
import com.teachme.core.network.FolderApi
import com.teachme.core.network.TopicApi
import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DefaultFolderRepository
    @Inject
    constructor(
        private val api: FolderApi,
    ) : FolderRepository {
        override suspend fun listFolders(): ApiResult<List<Folder>> = fetchAll { api.listFolders(it, PAGE_LIMIT) }

        override suspend fun getFolder(id: String) = api.getFolder(id)

        override suspend fun createFolder(name: String) = api.createFolder(name)

        override suspend fun renameFolder(
            id: String,
            name: String,
        ) = api.renameFolder(id, name)

        override suspend fun deleteFolder(id: String) = api.deleteFolder(id)
    }

@Singleton
class DefaultTopicRepository
    @Inject
    constructor(
        private val api: TopicApi,
    ) : TopicRepository {
        override suspend fun listTopics(folderId: String): ApiResult<List<Topic>> =
            fetchAll { api.listTopics(folderId, it, PAGE_LIMIT) }

        override suspend fun getTopic(id: String) = api.getTopic(id)

        override suspend fun createTopic(
            folderId: String,
            title: String,
            language: TopicLanguage,
        ) = api.createTopic(folderId, title, language)

        override suspend fun updateTopic(
            id: String,
            title: String?,
            folderId: String?,
        ) = api.updateTopic(id, title, folderId)

        override suspend fun deleteTopic(id: String) = api.deleteTopic(id)
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class RepositoryBindingsModule {
    @Binds
    abstract fun bindFolderRepository(impl: DefaultFolderRepository): FolderRepository

    @Binds
    abstract fun bindTopicRepository(impl: DefaultTopicRepository): TopicRepository

    @Binds
    abstract fun bindChatRepository(impl: DefaultChatRepository): ChatRepository

    @Binds
    abstract fun bindMissionRepository(impl: DefaultMissionRepository): com.teachme.core.model.MissionRepository

    @Binds
    abstract fun bindResourceRepository(impl: DefaultResourceRepository): com.teachme.core.model.ResourceRepository
}
