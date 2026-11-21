package com.teachme.app

import kotlinx.serialization.Serializable

/** Type-safe navigation routes (ADR 0001). */
@Serializable
data object HomeRoute

@Serializable
data class FolderRoute(
    val id: String,
    val name: String,
)

@Serializable
data class TopicRoute(
    val id: String,
    val title: String,
    /** A tab asked for by the screen that opens this one (`chat` from a lesson). */
    val tab: String? = null,
    /** Set when opened from a lesson: the question for the composer and the lesson as context (ask_agent.md). */
    val prompt: String? = null,
    val lessonId: String? = null,
    val lessonNumber: Int = 0,
    val lessonTitle: String? = null,
    val lessonMinutes: Int = 0,
)

/** Settings; [section] `llm-key` with [reason] `missing`/`rejected` is the deep link of the key notices (llm-key.md). */
@Serializable
data class SettingsRoute(
    val section: String? = null,
    val reason: String? = null,
)

/** The sign-in screen (login.md): first destination without a session, or over the stack when it ends. */
@Serializable
data object LoginRoute

@Serializable
data object ChangePasswordRoute

@Serializable
data object SuspendedRoute

@Serializable
data object MissingComponentsRoute

@Serializable
data object LicensesRoute

/** A lesson, by key: its id (a backend lesson) or `fixture/...` (a sample of a debug build). The field name is read by the ViewModel. */
@Serializable
data class LessonRoute(
    val key: String,
)

/**
 * The spaced-repetition session (review.md): full window on every width class. With [topicId] the session is limited to
 * one topic (review.md section 7.3); [topicTitle] is only for the TopBar. The field names are read by the ViewModel.
 */
@Serializable
data class ReviewRoute(
    val topicId: String? = null,
    val topicTitle: String? = null,
)
