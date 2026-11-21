package com.teachme.core.model

/** Small UI state that must survive an app restart (not a user setting). */
interface LibraryStateRepository {
    /** Id of the folder selected the last time the list-detail layout was used, if any. */
    suspend fun lastFolderId(): String?

    suspend fun setLastFolderId(id: String?)
}
