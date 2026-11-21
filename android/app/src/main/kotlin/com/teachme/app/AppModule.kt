package com.teachme.app

import com.teachme.core.model.AttemptQueue
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.PendingSync
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.network.AppVersion
import com.teachme.feature.lesson.AttemptSyncer
import com.teachme.feature.lesson.HighlightSyncer
import com.teachme.feature.lesson.StrokeSyncer
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object AppModule {
    @Provides
    fun provideAppVersion(): AppVersion = AppVersion(BuildConfig.VERSION_NAME)
}

@Module
@InstallIn(SingletonComponent::class)
abstract class AppBindingsModule {
    @Binds
    abstract fun bindPendingSync(impl: QueuesPendingSync): PendingSync
}

/** The review grades and the lesson answers waiting on this device, for the signed-in (or last) user. */
@Singleton
class QueuesPendingSync
    @Inject
    constructor(
        private val reviews: ReviewSyncer,
        private val attempts: AttemptSyncer,
        private val highlights: HighlightSyncer,
        private val strokes: StrokeSyncer,
        private val attemptQueue: AttemptQueue,
    ) : PendingSync {
        override suspend fun counts() =
            PendingCounts(
                reviews.pending().size,
                attemptQueue.pendingTotal(),
                highlights.pendingLessons(),
                strokes.pendingLessons(),
            )

        override suspend fun flush(timeoutMs: Long): PendingCounts {
            withTimeoutOrNull(timeoutMs) {
                coroutineScope {
                    launch { reviews.flush() }
                    launch { attempts.flushAll() }
                    launch { highlights.flushAll() }
                    launch { strokes.flushAll() }
                }
            }
            return counts()
        }
    }
