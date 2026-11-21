package com.teachme.core.model

/** Folders of the user. No offline cache in this phase: every call goes to the backend. */
interface FolderRepository {
    /** All folders in backend order, following every cursor page. */
    suspend fun listFolders(): ApiResult<List<Folder>>

    suspend fun getFolder(id: String): ApiResult<Folder>

    suspend fun createFolder(name: String): ApiResult<Folder>

    suspend fun renameFolder(
        id: String,
        name: String,
    ): ApiResult<Folder>

    /** Deletes the folder and all its Topics. */
    suspend fun deleteFolder(id: String): ApiResult<Unit>
}

interface TopicRepository {
    /** All Topics of a folder in backend order, following every cursor page. */
    suspend fun listTopics(folderId: String): ApiResult<List<Topic>>

    suspend fun getTopic(id: String): ApiResult<Topic>

    suspend fun createTopic(
        folderId: String,
        title: String,
        language: TopicLanguage = TopicLanguage.It,
    ): ApiResult<Topic>

    /** Renames and/or moves a Topic; a null argument leaves the field unchanged. */
    suspend fun updateTopic(
        id: String,
        title: String? = null,
        folderId: String? = null,
    ): ApiResult<Topic>

    suspend fun deleteTopic(id: String): ApiResult<Unit>
}
