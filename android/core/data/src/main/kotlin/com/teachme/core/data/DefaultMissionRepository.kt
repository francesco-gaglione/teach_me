package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.Mission
import com.teachme.core.model.MissionRepository
import com.teachme.core.model.Preference
import com.teachme.core.network.MissionApi
import javax.inject.Inject
import javax.inject.Singleton

/** Thin adapter: mission and preferences have no cache, every call goes to the backend. */
@Singleton
class DefaultMissionRepository
    @Inject
    constructor(
        private val api: MissionApi,
    ) : MissionRepository {
        override suspend fun getMission(topicId: String) = api.getMission(topicId)

        override suspend fun listVersions(topicId: String): ApiResult<List<Mission>> =
            fetchAll { api.listVersions(topicId, it, PAGE_LIMIT) }

        override suspend fun confirm(
            topicId: String,
            version: Int,
        ) = api.confirm(topicId, version)

        override suspend fun listPreferences(topicId: String): ApiResult<List<Preference>> =
            fetchAll { api.listPreferences(topicId, it, PAGE_LIMIT) }

        override suspend fun deletePreference(id: String) = api.deletePreference(id)
    }
