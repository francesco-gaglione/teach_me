package com.teachme.app

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import com.teachme.core.model.LessonDocumentSource
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

/** Release builds ship no sample lessons: there is nothing to show here until phase 6 brings the backend. */
@Composable
@Suppress("UNUSED_PARAMETER", "unused")
fun ColumnScope.DebugLessonsSection(onOpenLesson: (String) -> Unit) = Unit

@Module
@InstallIn(SingletonComponent::class)
object LessonSourceModule {
    @Provides
    fun provideLessonSource(): LessonDocumentSource =
        object : LessonDocumentSource {
            override suspend fun load(key: String): String? = null
        }
}
