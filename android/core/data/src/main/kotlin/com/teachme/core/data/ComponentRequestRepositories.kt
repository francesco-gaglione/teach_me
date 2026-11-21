package com.teachme.core.data

import com.teachme.core.model.ApiResult
import com.teachme.core.model.ComponentRequestGroup
import com.teachme.core.model.ComponentRequestRepository
import com.teachme.core.model.Page
import com.teachme.core.network.ComponentRequestsApi
import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DefaultComponentRequestRepository
    @Inject
    constructor(
        private val api: ComponentRequestsApi,
    ) : ComponentRequestRepository {
        // No cache: the screen reloads on every open (settings.md).
        override suspend fun list(cursor: String?): ApiResult<Page<ComponentRequestGroup>> = api.list(cursor)
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class ComponentRequestBindingsModule {
    @Binds
    abstract fun bindComponentRequestRepository(impl: DefaultComponentRequestRepository): ComponentRequestRepository
}
