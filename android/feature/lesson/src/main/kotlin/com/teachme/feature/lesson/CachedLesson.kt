package com.teachme.feature.lesson

import com.teachme.core.model.Lesson
import com.teachme.core.model.LessonStatus
import com.teachme.core.model.LessonView
import com.teachme.core.model.QualityWarning
import com.teachme.core.model.TopicLanguage
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * The lesson as last downloaded, kept on the device so that it opens without a network (lesson.md section 8.1:
 * "la lezione già scaricata si legge e svolge"). Only what the screen needs to open it: attempts and the closing state
 * are read again as soon as the backend answers.
 */
@Serializable
internal data class CachedLesson(
    val id: String,
    val topicId: String,
    val number: Int,
    val title: String,
    val objective: String,
    val estimatedMinutes: Int,
    val status: String,
    val catalogVersion: Int,
    val document: String,
    /** Absent from a cache written before contract v1.7: no notes. */
    val qualityWarnings: List<QualityWarning> = emptyList(),
    /** Wire code of the Topic language ("?" for one this app does not know, which reads back as unknown); absent before v1.9: Italian. */
    val language: String? = null,
) {
    fun toView() =
        LessonView(
            lesson =
                Lesson(
                    id = id,
                    topicId = topicId,
                    number = number,
                    title = title,
                    objective = objective,
                    estimatedMinutes = estimatedMinutes,
                    status = LessonStatus.fromWire(status),
                    catalogVersion = catalogVersion,
                    missionVersion = null,
                    createdAt = null,
                    completedAt = null,
                ),
            documentJson = document,
            attempts = emptyList(),
            completion = null,
            qualityWarnings = qualityWarnings,
            language = TopicLanguage.fromWire(language),
        )

    fun encode(): String = JSON.encodeToString(serializer(), this)

    companion object {
        private val JSON = Json { ignoreUnknownKeys = true }

        fun of(view: LessonView) =
            CachedLesson(
                id = view.lesson.id,
                topicId = view.lesson.topicId,
                number = view.lesson.number,
                title = view.lesson.title,
                objective = view.lesson.objective,
                estimatedMinutes = view.lesson.estimatedMinutes,
                status = view.lesson.status.wire,
                catalogVersion = view.lesson.catalogVersion,
                document = view.documentJson,
                qualityWarnings = view.qualityWarnings,
                language = view.language.wire ?: "?",
            )

        /** Never throws: damaged data is no cache. */
        fun decode(text: String?): CachedLesson? =
            if (text.isNullOrBlank()) null else runCatching { JSON.decodeFromString(serializer(), text) }.getOrNull()
    }
}
