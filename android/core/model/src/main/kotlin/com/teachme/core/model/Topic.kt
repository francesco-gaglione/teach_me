package com.teachme.core.model

import java.time.Instant

/** A learning topic inside a folder. */
data class Topic(
    val id: String,
    val folderId: String,
    val title: String,
    val state: TopicState,
    val createdAt: Instant,
    val updatedAt: Instant,
    /** Language of everything generated for the Topic; chosen at creation, never changes (ADR 0019). */
    val language: TopicLanguage = TopicLanguage.It,
)

/** Content language of a Topic (`folders-topics.md` section 1). */
enum class TopicLanguage(
    val wire: String?,
) {
    It("it"),
    En("en"),

    /** A language this app does not know (a newer backend): never offered when creating a Topic. */
    Unknown(null),
    ;

    companion object {
        /** A missing value (an old server) is read as Italian, a value this app does not know as [Unknown]. */
        fun fromWire(value: String?): TopicLanguage =
            if (value == null) It else entries.firstOrNull { it.wire == value } ?: Unknown

        /** The languages a person can choose when creating a Topic. */
        val selectable: List<TopicLanguage> get() = listOf(It, En)

        /** The language offered when creating a Topic: the device language when it is English, else Italian. */
        fun defaultFor(deviceLanguage: String?): TopicLanguage =
            if (deviceLanguage.equals(En.wire, ignoreCase = true)) En else It
    }
}

/** Lifecycle of a Topic (`docs/mvp.md` §5). Values added by the backend later map to [Unknown]. */
enum class TopicState(
    val wire: String?,
) {
    New("new"),
    Interviewing("interviewing"),
    MissionProposed("mission_proposed"),
    Researching("researching"),
    Ready("ready"),
    Unknown(null),
    ;

    companion object {
        fun fromWire(value: String?): TopicState =
            entries.firstOrNull { it.wire != null && it.wire == value } ?: Unknown
    }
}
