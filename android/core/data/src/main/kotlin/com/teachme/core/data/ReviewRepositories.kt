package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.AppClock
import com.teachme.core.model.LegacyDataClaimer
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewAlertReference
import com.teachme.core.model.ReviewQueue
import com.teachme.core.model.ReviewReminderPrefs
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.UserScope
import com.teachme.core.model.UserScopes
import com.teachme.core.network.ReviewsApi
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

/** Thin adapter: reviews have no cache, every call goes to the backend. */
@Singleton
class DefaultReviewRepository
    @Inject
    constructor(
        private val api: ReviewsApi,
    ) : ReviewRepository {
        override suspend fun due(
            limit: Int,
            topicId: String?,
        ) = api.due(limit, topicId)

        override suspend fun rate(review: QueuedReview) = api.rate(review)

        override suspend fun undo(cardId: String) = api.undo(cardId)

        override suspend fun suspend(cardId: String) = api.suspend(cardId)

        override suspend fun unsuspend(cardId: String) = api.unsuspend(cardId)

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ) = api.suspended(limit, cursor)
    }

/** The grades not yet registered by the backend: one persistent list, oldest first, see [JsonListStore]. */
class DataStoreReviewQueue(
    store: DataStore<Preferences>,
    scopes: UserScopes,
) : ReviewQueue {
    private val list = JsonListStore(store, REVIEWS_KEY, QueuedReview.serializer(), scopes)

    override suspend fun enqueue(review: QueuedReview) =
        list.change { items ->
            val index = items.indexOfFirst { it.sameReview(review) }
            if (index < 0) items + review else items.toMutableList().also { it[index] = review }
        }

    override suspend fun pending(): List<QueuedReview> = list.read()

    override suspend fun remove(review: QueuedReview): Boolean {
        var removed = false
        list.change { items ->
            removed = items.any { it.sameReview(review) }
            items.filterNot { it.sameReview(review) }
        }
        return removed
    }

    override fun count(): Flow<Int> = list.observe().map { it.size }.distinctUntilChanged()
}

internal const val REVIEWS_KEY = "reviews"

internal val Context.reviewQueueDataStore: DataStore<Preferences> by preferencesDataStore(name = "review_queue")

@Module
@InstallIn(SingletonComponent::class)
abstract class ReviewBindingsModule {
    @Binds
    abstract fun bindReviewRepository(impl: DefaultReviewRepository): ReviewRepository
}

@Module
@InstallIn(SingletonComponent::class)
object ReviewProvidesModule {
    @Provides
    @Singleton
    fun provideReviewQueue(
        @ApplicationContext context: Context,
        scopes: UserScopes,
    ): ReviewQueue = DataStoreReviewQueue(context.reviewQueueDataStore, scopes)

    @Provides
    @Singleton
    fun provideClock(): AppClock = AppClock { System.currentTimeMillis() }

    @Provides
    @Singleton
    fun provideReviewSyncer(
        repository: ReviewRepository,
        queue: ReviewQueue,
        scopes: UserScopes,
    ): ReviewSyncer = ReviewSyncer(repository, queue, scopes)

    @Provides
    @Singleton
    fun provideLegacyDataClaimer(
        @ApplicationContext context: Context,
    ): LegacyDataClaimer =
        LegacyDataClaimer { scope ->
            claimLegacyList(context.reviewQueueDataStore, REVIEWS_KEY, scope)
            claimLegacyList(context.attemptQueueDataStore, ATTEMPTS_KEY, scope)
            // Progress and downloaded lessons were kept under the plain lesson key.
            claimLegacyKeys(context.lessonDataStore, scope) { !it.contains(SCOPE_SEPARATOR) }
            claimLegacyKeys(context.documentBannerDataStore, scope) { it == DISMISSED_KEY }
        }
}

/** The last notified count of due cards, one preference per user ([scopedKeyName]), see [ReviewAlertReference]. */
class DataStoreReviewAlertReference(
    private val store: DataStore<Preferences>,
) : ReviewAlertReference {
    private fun key(scope: UserScope) = intPreferencesKey(scopedKeyName(ALERT_REFERENCE_KEY, scope))

    override suspend fun get(scope: UserScope): Int = store.data.first()[key(scope)] ?: 0

    override suspend fun set(
        scope: UserScope,
        count: Int,
    ) {
        store.edit { if (count <= 0) it.remove(key(scope)) else it[key(scope)] = count }
    }
}

internal const val ALERT_REFERENCE_KEY = "review_alert_reference"

internal val Context.reviewAlertDataStore: DataStore<Preferences> by preferencesDataStore(name = "review_alerts")

@Module
@InstallIn(SingletonComponent::class)
object ReviewAlertModule {
    @Provides
    @Singleton
    fun provideReviewAlertReference(
        @ApplicationContext context: Context,
    ): ReviewAlertReference = DataStoreReviewAlertReference(context.reviewAlertDataStore)
}

/** The device-only switch of the review notifications and whether the permission was asked (ADR 0012 points 3, 6). */
class DataStoreReviewReminderPrefs(
    private val store: DataStore<Preferences>,
) : ReviewReminderPrefs {
    // Absent means on: the switch is on by default.
    override val enabled: Flow<Boolean> = store.data.map { it[KEY_ENABLED] ?: true }.distinctUntilChanged()

    override suspend fun isEnabled(): Boolean = store.data.first()[KEY_ENABLED] ?: true

    override suspend fun setEnabled(enabled: Boolean) {
        store.edit { it[KEY_ENABLED] = enabled }
    }

    override suspend fun permissionAsked(): Boolean = store.data.first()[KEY_ASKED] ?: false

    override suspend fun markPermissionAsked() {
        store.edit { it[KEY_ASKED] = true }
    }

    private companion object {
        val KEY_ENABLED = booleanPreferencesKey("review_notifications_enabled")
        val KEY_ASKED = booleanPreferencesKey("notification_permission_asked")
    }
}

internal val Context.reviewReminderDataStore: DataStore<Preferences> by preferencesDataStore(name = "review_reminders")

@Module
@InstallIn(SingletonComponent::class)
object ReviewReminderPrefsModule {
    @Provides
    @Singleton
    fun provideReviewReminderPrefs(
        @ApplicationContext context: Context,
    ): ReviewReminderPrefs = DataStoreReviewReminderPrefs(context.reviewReminderDataStore)
}
