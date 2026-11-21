package com.teachme.app

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachListCard
import com.teachme.core.designsystem.components.TeachListRow
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.model.LessonDocumentSource
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive

private const val FIXTURES_DIR = "fixtures"
private const val KEY_PREFIX = "fixture/"

/** Debug builds serve the contract fixtures bundled in the assets (see CopyLessonFixtures in build.gradle.kts). */
private class FixtureLessonSource(
    private val context: Context,
) : LessonDocumentSource {
    override suspend fun load(key: String): String? =
        withContext(Dispatchers.IO) {
            val name =
                key.removePrefix(KEY_PREFIX).takeIf { key.startsWith(KEY_PREFIX) && '/' !in it }
                    ?: return@withContext null
            runCatching {
                context.assets
                    .open(
                        "$FIXTURES_DIR/$name",
                    ).bufferedReader()
                    .use { it.readText() }
            }.getOrNull()
        }
}

@Module
@InstallIn(SingletonComponent::class)
object LessonSourceModule {
    @Provides
    fun provideLessonSource(
        @ApplicationContext context: Context,
    ): LessonDocumentSource = FixtureLessonSource(context)
}

private data class FixtureEntry(
    val key: String,
    val title: String,
    val file: String,
)

private suspend fun listFixtures(context: Context): List<FixtureEntry> =
    withContext(Dispatchers.IO) {
        context.assets
            .list(FIXTURES_DIR)
            .orEmpty()
            .filter { it.endsWith(".json") }
            .sorted()
            .map { file ->
                val title =
                    runCatching {
                        val text =
                            context.assets
                                .open("$FIXTURES_DIR/$file")
                                .bufferedReader()
                                .use { it.readText() }
                        (Json.parseToJsonElement(text) as JsonObject)["title"]?.jsonPrimitive?.content
                    }.getOrNull() ?: file
                FixtureEntry(KEY_PREFIX + file, title, file)
            }
    }

/** Settings section (debug builds only) that opens the bundled sample lessons. */
@Composable
fun ColumnScope.DebugLessonsSection(onOpenLesson: (String) -> Unit) {
    val context = LocalContext.current
    val entries by produceState(emptyList<FixtureEntry>()) { value = listFixtures(context) }
    if (entries.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.xs)) {
        TeachText(stringResource(R.string.debug_lessons_title), style = TeachTheme.typography.ui.title3)
        TeachText(
            stringResource(R.string.debug_lessons_body),
            style = TeachTheme.typography.ui.body,
            color = TeachTheme.colors.inkSoft,
        )
        TeachListCard {
            Column {
                entries.forEach { entry ->
                    TeachListRow(
                        title = entry.title,
                        subtitle = entry.file,
                        onClick = { onOpenLesson(entry.key) },
                        contentDescription = stringResource(R.string.debug_lessons_open, entry.title, entry.file),
                    )
                }
            }
        }
    }
}
