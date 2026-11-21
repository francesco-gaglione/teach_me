package com.teachme.core.data

import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.ResourceRepository
import com.teachme.core.model.observeJob
import com.teachme.core.network.ResourcesApi
import javax.inject.Inject
import javax.inject.Singleton

/** Thin adapter: sources have no cache, every call goes to the backend. A job that comes in is reported to [llmKey]. */
@Singleton
class DefaultResourceRepository
    @Inject
    constructor(
        private val api: ResourcesApi,
        private val llmKey: LlmKeyMonitor,
    ) : ResourceRepository {
        override suspend fun getResources(topicId: String) =
            api.getResources(topicId).alsoOnSuccess { view -> view.job?.let(llmKey::observeJob) }

        override suspend fun startResearch(topicId: String) =
            api.startResearch(topicId).alsoOnSuccess(llmKey::observeJob)

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ) = api.retryJob(topicId, jobId).alsoOnSuccess(llmKey::observeJob)
    }
