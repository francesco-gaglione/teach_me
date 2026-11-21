package com.teachme.core.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.teachme.core.model.ApiResult
import com.teachme.core.model.Attempt
import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.GlossaryTerm
import com.teachme.core.model.HighlightRecord
import com.teachme.core.model.HighlightStore
import com.teachme.core.model.InMemoryTopicTabInbox
import com.teachme.core.model.LearningRecord
import com.teachme.core.model.LessonInProgress
import com.teachme.core.model.LessonRepository
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.OPEN_ANSWER_CLIENT_TIMEOUT_MS
import com.teachme.core.model.QueuedAttempt
import com.teachme.core.model.StoredHighlights
import com.teachme.core.model.StrokeRecord
import com.teachme.core.model.StrokeStore
import com.teachme.core.model.TopicTabInbox
import com.teachme.core.model.UserScope
import com.teachme.core.model.UserScopes
import com.teachme.core.model.observeJob
import com.teachme.core.network.LessonsApi
import com.teachme.core.network.ResourcesApi
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

/** Page size of the lessons list (the tab loads more when the end is near). */
internal const val LESSONS_PAGE_LIMIT = 20

/** Thin adapter: lessons have no cache, every call goes to the backend. A job that comes in is reported to [llmKey]. */
@Singleton
class DefaultLessonRepository
    @Inject
    constructor(
        private val api: LessonsApi,
        private val resources: ResourcesApi,
        private val llmKey: LlmKeyMonitor,
    ) : LessonRepository {
        override suspend fun startGeneration(topicId: String) =
            api.startGeneration(topicId).alsoOnSuccess(llmKey::observeJob)

        override suspend fun listLessons(
            topicId: String,
            cursor: String?,
        ) = api.listLessons(topicId, cursor, LESSONS_PAGE_LIMIT)

        override suspend fun getLesson(lessonId: String) =
            api.getLesson(lessonId).alsoOnSuccess { view -> view.completion?.job?.let(llmKey::observeJob) }

        override suspend fun lastInProgress(): ApiResult<LessonInProgress?> =
            when (val result = api.listInProgress(1)) {
                is ApiResult.Success -> ApiResult.Success(result.value.items.firstOrNull())
                is ApiResult.Failure -> result
            }

        override suspend fun submitAttempts(
            lessonId: String,
            attempts: List<QueuedAttempt>,
        ): ApiResult<List<Attempt>> =
            api.submitAttempts(
                lessonId,
                attempts,
                timeoutMs = if (attempts.any { it.isOpenAnswer }) OPEN_ANSWER_CLIENT_TIMEOUT_MS else null,
            )

        override suspend fun completeLesson(lessonId: String) =
            api.completeLesson(lessonId).alsoOnSuccess(llmKey::observeJob)

        override suspend fun putHighlights(
            lessonId: String,
            highlights: List<HighlightRecord>,
        ) = api.putHighlights(lessonId, highlights)

        override suspend fun putStrokes(
            lessonId: String,
            strokes: List<StrokeRecord>,
        ) = api.putStrokes(lessonId, strokes)

        override suspend fun listJobs(topicId: String) =
            fetchAll { api.listJobs(topicId, it, JOBS_PAGE_LIMIT) }.alsoOnSuccess { it.forEach(llmKey::observeJob) }

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ) = resources.retryJob(topicId, jobId).alsoOnSuccess(llmKey::observeJob)

        override suspend fun listGlossary(topicId: String): ApiResult<List<GlossaryTerm>> =
            fetchAll { api.listGlossary(topicId, it, PAGE_LIMIT) }

        override suspend fun listRecords(topicId: String): ApiResult<List<LearningRecord>> =
            fetchAll { api.listRecords(topicId, it, PAGE_LIMIT) }
    }

/** Only the most recent jobs matter (the current job of a kind is the first of the list). */
private const val JOBS_PAGE_LIMIT = 50

/** The attempts not yet registered by the backend: one persistent list, see [JsonListStore]. */
class DataStoreAttemptQueue(
    store: DataStore<Preferences>,
    scopes: UserScopes,
) : AttemptQueue {
    private val list = JsonListStore(store, ATTEMPTS_KEY, QueuedAttempt.serializer(), scopes)

    override suspend fun enqueue(attempt: QueuedAttempt) =
        list.change { items ->
            val index = items.indexOfFirst { it.sameAttempt(attempt) }
            if (index < 0) items + attempt else items.toMutableList().also { it[index] = attempt }
        }

    override suspend fun pending(lessonId: String): List<QueuedAttempt> = list.read().filter { it.lessonId == lessonId }

    override suspend fun lessonsWithPending(): List<String> = list.read().map { it.lessonId }.distinct()

    override suspend fun pendingTotal(): Int = list.read().size

    override suspend fun remove(attempts: List<QueuedAttempt>) =
        list.change { items -> items.filter { q -> attempts.none { it.sameAttempt(q) } } }

    override suspend fun clear(lessonId: String) = list.change { items -> items.filter { it.lessonId != lessonId } }

    override fun count(lessonId: String): Flow<Int> =
        list.observe().map { items -> items.count { it.lessonId == lessonId } }.distinctUntilChanged()
}

/**
 * The highlights of every lesson not yet sent (and the last ones the backend gave, for opening offline), plus the colour
 * the pen used last: one [JsonListStore] entry per lesson, and the colour under its own key, both per user (ADR 0008).
 */
class DataStoreHighlightStore(
    private val store: DataStore<Preferences>,
    private val scopes: UserScopes,
) : HighlightStore {
    private val list = JsonListStore(store, HIGHLIGHTS_KEY, StoredHighlights.serializer(), scopes)

    override suspend fun get(lessonId: String): StoredHighlights? = list.read().firstOrNull { it.lessonId == lessonId }

    override suspend fun update(
        lessonId: String,
        transform: (StoredHighlights?) -> StoredHighlights?,
    ) = list.change { items ->
        val old = items.firstOrNull { it.lessonId == lessonId }
        val new = transform(old)
        val rest = items.filter { it.lessonId != lessonId }
        when {
            new == null -> rest
            // A copy of the backend's empty set says nothing more than having no entry.
            !new.dirty && new.items.isEmpty() -> rest
            // Nothing changed (the server's set is the one already kept): no write, no new place in the order.
            old != null && old == new.copy(lessonId = lessonId) -> items
            else -> prune(rest + new.copy(lessonId = lessonId))
        }
    }

    /**
     * The list is in order of last change. Sets with unsent changes are never dropped; of the others (copies of the
     * backend's set kept to open a lesson offline) only the [MAX_CLEAN_HIGHLIGHT_SETS] most recent stay.
     */
    private fun prune(items: List<StoredHighlights>): List<StoredHighlights> {
        var clean = items.count { !it.dirty }
        if (clean <= MAX_CLEAN_HIGHLIGHT_SETS) return items
        return items.filter { it.dirty || (clean-- <= MAX_CLEAN_HIGHLIGHT_SETS) }
    }

    override suspend fun dirtyLessons(): List<String> = list.read().filter { it.dirty }.map { it.lessonId }

    private fun colorKey(scope: UserScope) = stringPreferencesKey(scopedKeyName(HIGHLIGHT_COLOR_KEY, scope))

    override suspend fun color(): String? {
        val scope = scopes.current() ?: return null
        return store.data.first()[colorKey(scope)]
    }

    override suspend fun setColor(color: String) {
        val scope = scopes.current() ?: return
        store.edit { it[colorKey(scope)] = color }
    }
}

/** How many lessons keep a copy of their highlights that has already been sent (the rest is read from the backend). */
internal const val MAX_CLEAN_HIGHLIGHT_SETS = 30

internal const val HIGHLIGHTS_KEY = "highlights"
internal const val HIGHLIGHT_COLOR_KEY = "highlight_color"

internal val Context.highlightDataStore: DataStore<Preferences> by preferencesDataStore(name = "highlights")

internal const val ATTEMPTS_KEY = "queue"

internal val Context.attemptQueueDataStore: DataStore<Preferences> by preferencesDataStore(name = "attempt_queue")

@Module
@InstallIn(SingletonComponent::class)
abstract class LessonBindingsModule {
    @Binds
    abstract fun bindLessonRepository(impl: DefaultLessonRepository): LessonRepository
}

@Module
@InstallIn(SingletonComponent::class)
object LessonProvidesModule {
    @Provides
    @Singleton
    fun provideAttemptQueue(
        @ApplicationContext context: Context,
        scopes: UserScopes,
    ): AttemptQueue = DataStoreAttemptQueue(context.attemptQueueDataStore, scopes)

    @Provides
    @Singleton
    fun provideHighlightStore(
        @ApplicationContext context: Context,
        scopes: UserScopes,
    ): HighlightStore = DataStoreHighlightStore(context.highlightDataStore, scopes)

    @Provides
    @Singleton
    fun provideStrokeStore(
        @ApplicationContext context: Context,
        scopes: UserScopes,
    ): StrokeStore = DataStoreStrokeStore(context.strokeDataStore, scopes, context.filesDir)

    @Provides
    @Singleton
    fun provideTopicTabInbox(): TopicTabInbox = InMemoryTopicTabInbox()
}
