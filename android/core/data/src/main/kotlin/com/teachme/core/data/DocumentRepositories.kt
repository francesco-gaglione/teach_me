package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.DocumentBannerStore
import com.teachme.core.model.DocumentRepository
import com.teachme.core.model.UploadSource
import com.teachme.core.model.map
import com.teachme.core.network.DocumentsApi
import com.teachme.core.network.ResourcesApi
import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.flow.first
import javax.inject.Inject
import javax.inject.Singleton

/** Thin adapter: documents have no cache, every call goes to the backend. */
@Singleton
class DefaultDocumentRepository
    @Inject
    constructor(
        private val api: DocumentsApi,
        private val resourcesApi: ResourcesApi,
    ) : DocumentRepository {
        override suspend fun list(topicId: String) = api.list(topicId)

        override suspend fun upload(
            topicId: String,
            source: UploadSource,
        ) = api.upload(topicId, source)

        override suspend fun delete(documentId: String) = api.delete(documentId)

        override suspend fun importJobs(topicId: String) =
            api.listJobs(topicId).map { page -> page.items.filter { it.isDocumentImport } }

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ) = resourcesApi.retryJob(topicId, jobId)
    }

internal val Context.documentBannerDataStore: DataStore<Preferences> by preferencesDataStore(name = "document_banner")

internal const val DISMISSED_KEY = "dismissed_document_ids"

private fun dismissedKey(scope: com.teachme.core.model.UserScope) =
    stringSetPreferencesKey("$DISMISSED_KEY$SCOPE_SEPARATOR${scope.key}")

@Singleton
class DataStoreDocumentBannerStore
    @Inject
    constructor(
        @ApplicationContext private val context: Context,
        private val scopes: com.teachme.core.model.UserScopes,
    ) : DocumentBannerStore {
        override suspend fun dismissed(): Set<String> {
            val owner = scopes.current() ?: return emptySet()
            return runCatching {
                context.documentBannerDataStore.data.first()[
                    dismissedKey(
                        owner,
                    ),
                ]
            }.getOrNull().orEmpty()
        }

        override suspend fun dismiss(documentIds: Collection<String>) {
            if (documentIds.isEmpty()) return
            val owner = scopes.current() ?: return
            val key = dismissedKey(owner)
            runCatching {
                context.documentBannerDataStore.edit { prefs ->
                    prefs[key] = prefs[key].orEmpty() + documentIds
                }
            }
        }
    }

@Module
@InstallIn(SingletonComponent::class)
abstract class DocumentBindingsModule {
    @Binds
    abstract fun bindDocumentRepository(impl: DefaultDocumentRepository): DocumentRepository

    @Binds
    abstract fun bindDocumentBannerStore(impl: DataStoreDocumentBannerStore): DocumentBannerStore
}
