package com.teachme.core.network

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.TopicJob
import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.http.path
import javax.inject.Inject
import javax.inject.Singleton

/** Raw endpoints of `docs/contracts/resources.md` section 3 and `jobs.md` section 3. */
interface ResourcesApi {
    suspend fun getResources(topicId: String): ApiResult<ResourcesView>

    suspend fun startResearch(topicId: String): ApiResult<TopicJob>

    suspend fun retryJob(
        topicId: String,
        jobId: String,
    ): ApiResult<TopicJob>
}

@Singleton
class KtorResourcesApi
    @Inject
    constructor(
        private val client: HttpClient,
    ) : ResourcesApi {
        override suspend fun getResources(topicId: String): ApiResult<ResourcesView> =
            apiCall({ client.get { url { path("api", "v1", "topics", topicId, "resources") } } }) {
                it.body<ResourcesViewDto>().toModel()
            }

        override suspend fun startResearch(topicId: String): ApiResult<TopicJob> =
            apiCall({ client.post { url { path("api", "v1", "topics", topicId, "resources", "research") } } }) {
                it.body<JobDto>().toModel()
            }

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ): ApiResult<TopicJob> =
            apiCall({ client.post { url { path("api", "v1", "topics", topicId, "jobs", jobId, "retry") } } }) {
                it.body<JobDto>().toModel()
            }
    }
