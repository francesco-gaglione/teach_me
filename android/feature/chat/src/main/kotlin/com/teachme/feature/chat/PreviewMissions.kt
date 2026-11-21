package com.teachme.feature.chat

import com.teachme.core.model.Mission
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.ResearchPointer
import com.teachme.core.model.TopicMission
import com.teachme.feature.mission.ConfirmState
import com.teachme.feature.mission.MissionChatContext
import java.time.Instant

/** Sample missions for the chat previews. */
internal object PreviewMissions {
    val confirmed =
        Mission(
            "t",
            1,
            MissionStatus.Confirmed,
            "Mantenere il backend di pagamenti scritto in Rust nel mio nuovo lavoro.",
            listOf("Leggo e modifico un servizio axum esistente senza aiuto", "Scrivo test per un modulo con tokio"),
            listOf("30 minuti al giorno"),
            listOf("Rust embedded"),
            Instant.parse("2026-11-03T10:00:00Z"),
            Instant.parse("2026-11-03T10:05:00Z"),
            null,
        )
    val draft =
        confirmed.copy(
            version = 2,
            status = MissionStatus.Draft,
            successCriteria =
                listOf(
                    "Leggo e modifico un servizio axum esistente senza aiuto",
                    "Scrivo test di integrazione",
                ),
            confirmedAt = null,
        )
}

internal fun previewMissionContext(
    mission: TopicMission? = TopicMission(null, null),
    confirm: ConfirmState = ConfirmState.Idle,
    turnInProgress: Boolean = false,
    research: ResearchPointer = ResearchPointer.None,
) = MissionChatContext("Rust", mission, confirm, turnInProgress, research, {}, {}, {}, {})
