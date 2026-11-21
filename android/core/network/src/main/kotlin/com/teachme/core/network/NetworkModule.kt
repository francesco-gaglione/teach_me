package com.teachme.core.network

import com.teachme.core.model.SettingsRepository
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
abstract class NetworkBindingsModule {
    @Binds
    abstract fun bindHealthApi(impl: KtorHealthApi): HealthApi

    @Binds
    abstract fun bindAuthApi(impl: KtorAuthApi): AuthApi

    @Binds
    abstract fun bindFolderApi(impl: KtorFolderApi): FolderApi

    @Binds
    abstract fun bindTopicApi(impl: KtorTopicApi): TopicApi

    @Binds
    abstract fun bindChatApi(impl: KtorChatApi): ChatApi

    @Binds
    abstract fun bindMissionApi(impl: KtorMissionApi): MissionApi

    @Binds
    abstract fun bindResourcesApi(impl: KtorResourcesApi): ResourcesApi

    @Binds
    abstract fun bindDocumentsApi(impl: KtorDocumentsApi): DocumentsApi

    @Binds
    abstract fun bindLessonsApi(impl: KtorLessonsApi): LessonsApi

    @Binds
    abstract fun bindAudioApi(impl: KtorAudioApi): AudioApi

    @Binds
    abstract fun bindReviewsApi(impl: KtorReviewsApi): ReviewsApi

    @Binds
    abstract fun bindComponentRequestsApi(impl: KtorComponentRequestsApi): ComponentRequestsApi
}

@Module
@InstallIn(SingletonComponent::class)
object NetworkProvidesModule {
    @Provides
    @Singleton
    fun provideHttpClient(
        settings: SettingsRepository,
        appVersion: AppVersion,
        unauthorized: UnauthorizedHandler,
    ): HttpClient = createBackendHttpClient(OkHttp.create(), settings, appVersion, unauthorized)
}
