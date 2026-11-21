package com.teachme.feature.mission

import com.teachme.core.model.Mission
import com.teachme.core.model.MissionStatus
import com.teachme.core.model.Preference
import com.teachme.core.model.PreferenceScope
import java.time.Instant

/** Sample data for Compose previews and tests. */
internal object PreviewData {
    const val TOPIC = "t1"

    val confirmed1 =
        Mission(
            topicId = TOPIC,
            version = 1,
            status = MissionStatus.Confirmed,
            why = "Mantenere il backend di pagamenti scritto in Rust nel mio nuovo lavoro.",
            startingPoint =
                "Programmo in Kotlin da tre anni e ho scritto un po' di C; di Rust ho letto solo i primi capitoli " +
                    "del libro ufficiale, mai scritto codice.",
            successCriteria =
                listOf(
                    "Leggo e modifico un servizio axum esistente senza aiuto",
                    "Scrivo test per un modulo con tokio",
                    "Capisco i messaggi del borrow checker",
                ),
            constraints = listOf("30 minuti al giorno", "So già Kotlin e un po' di C"),
            outOfScope = listOf("Rust embedded"),
            createdAt = Instant.parse("2026-11-03T10:00:00Z"),
            confirmedAt = Instant.parse("2026-11-03T10:05:00Z"),
            supersededAt = null,
        )

    val draft2 =
        confirmed1.copy(
            version = 2,
            status = MissionStatus.Draft,
            why = "Mantenere e far evolvere il backend di pagamenti in Rust nel mio nuovo lavoro.",
            startingPoint =
                "Programmo in Kotlin da tre anni e ho scritto un po' di C. Di Rust ho finito il libro ufficiale e " +
                    "scritto due piccoli programmi da riga di comando, ma non ho mai usato async né tokio.\n" +
                    "Non ho mai visto sqlx.",
            successCriteria =
                listOf(
                    "Leggo e modifico un servizio axum esistente senza aiuto",
                    "Scrivo test di integrazione con tokio e sqlx",
                ),
            constraints = listOf("30 minuti al giorno", "So già Kotlin e un po' di C"),
            outOfScope = listOf("Rust embedded", "Macro procedurali"),
            createdAt = Instant.parse("2026-11-08T14:36:00Z"),
            confirmedAt = null,
        )

    /** A mission saved before contract v1.2: no starting point. */
    val confirmedNoStart = confirmed1.copy(startingPoint = null)

    /** A change proposal that adds the starting point to [confirmedNoStart]. */
    val draftAddsStart = draft2.copy(why = confirmedNoStart.why, successCriteria = confirmedNoStart.successCriteria)

    /** First proposal, with no confirmed version before it. */
    val first =
        confirmed1.copy(
            status = MissionStatus.Draft,
            createdAt = Instant.parse("2026-11-08T14:36:00Z"),
            confirmedAt = null,
        )

    val prefTopic =
        Preference(
            "p1",
            PreferenceScope.Topic,
            TOPIC,
            "Esempi in Python, non in Java",
            Instant.parse("2026-11-08T14:00:00Z"),
        )
    val prefGlobal =
        Preference(
            "p2",
            PreferenceScope.Global,
            null,
            "Niente analogie sportive",
            Instant.parse("2026-11-08T14:10:00Z"),
        )
}
