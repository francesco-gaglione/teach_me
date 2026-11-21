package com.teachme.feature.resources

import com.teachme.core.model.DocumentError
import com.teachme.core.model.DocumentSection
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.Gap
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.Resource
import com.teachme.core.model.ResourceKind
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.SourceType
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import java.time.Instant

/** Sample data for previews. */
internal object PreviewData {
    private val at: Instant = Instant.parse("2026-11-09T14:40:12Z")

    private fun resource(
        id: String,
        kind: ResourceKind,
        type: SourceType,
        title: String,
        url: String?,
        domain: String?,
        annotation: String,
        useFor: String,
    ) = Resource(id, kind, type, title, url, domain, "en", annotation, useFor, at)

    val resources =
        listOf(
            resource(
                "1",
                ResourceKind.Knowledge,
                SourceType.Book,
                "The Rust Programming Language",
                "https://doc.rust-lang.org/book/",
                "doc.rust-lang.org",
                "Il libro ufficiale, mantenuto dal team di Rust: copre ownership e borrowing con esempi completi.",
                "capire ownership e borrow prima di leggere il codice del servizio",
            ),
            resource(
                "2",
                ResourceKind.Knowledge,
                SourceType.Documentation,
                "Axum documentation",
                "https://docs.rs/axum/latest/axum/",
                "docs.rs",
                "Riferimento dell'API del framework web usato nel servizio.",
                "leggere gli handler e gli extractor del backend dei pagamenti",
            ),
            resource(
                "3",
                ResourceKind.Community,
                SourceType.LiveClass,
                "Rust Milano meetup",
                null,
                null,
                "Incontri mensili dal vivo con sviluppatori che usano Rust in produzione.",
                "fare domande a chi mantiene servizi Rust in azienda",
            ),
            resource(
                "4",
                ResourceKind.Community,
                SourceType.Forum,
                "users.rust-lang.org",
                "https://users.rust-lang.org/",
                "users.rust-lang.org",
                "Forum ufficiale per domande di chi sta imparando.",
                "chiedere aiuto sugli errori del borrow checker",
            ),
        )

    val gaps =
        listOf(
            Gap("Migrazioni con sqlx in produzione", "Nessuna guida affidabile e aggiornata."),
            Gap("Esempi di test d'integrazione con sqlx", "Solo post di blog datati."),
        )

    val research = ResearchInfo("job1", 2, at)

    val done =
        ResourcesView(resources, gaps, communitiesSkipped = false, research = research, job = job(JobStatus.Succeeded))

    val noSources = ResourcesView(emptyList(), gaps, false, research, job(JobStatus.Succeeded))

    val skipped =
        ResourcesView(resources.filter { it.kind == ResourceKind.Knowledge }, emptyList(), true, research, null)

    fun job(
        status: JobStatus,
        step: String? = null,
        message: String? = null,
        errorCode: String? = null,
    ) = TopicJob(
        id = "job1",
        topicId = "t1",
        kind = "research",
        status = status,
        progress = if (step != null || message != null) JobProgress(step, message) else null,
        error = errorCode?.let { JobError(it, "upstream returned 529 after 3 attempts") },
        result = if (status == JobStatus.Succeeded) JobResult(resources.size, gaps.size) else null,
        attempt = 1,
        createdAt = at,
        startedAt = at,
        finishedAt = if (status == JobStatus.Succeeded || status == JobStatus.Failed) at else null,
    )

    private val later: Instant = Instant.parse("2026-11-10T09:00:00Z")

    fun document(
        id: String = "d1",
        status: DocumentStatus = DocumentStatus.Ready,
        title: String = "Rust in Action",
        filename: String = "rust-in-action.pdf",
        errorCode: String? = null,
        language: String? = "en",
        sections: Int = 4,
        createdAt: Instant = later,
    ) = TopicDocument(
        id = id,
        topicId = "t1",
        resourceId = if (status == DocumentStatus.Ready) "r-$id" else null,
        filename = filename,
        title = title,
        status = status,
        error = errorCode?.let { DocumentError(it, "Il PDF non contiene testo selezionabile.") },
        sizeBytes = 8_421_337,
        pageCount = if (status == DocumentStatus.Ready) 456 else null,
        language = language,
        sections =
            if (status == DocumentStatus.Ready) {
                listOf(
                    DocumentSection("s1", "1 Introducing Rust", 1, 1, 24),
                    DocumentSection("s2", "1.1 Why Rust", 2, 3, 9),
                    DocumentSection("s3", "2 Language foundations", 1, 25, 25),
                    DocumentSection("s4", "Pagine 26-35", 1, 26, 35),
                ).take(sections)
            } else {
                emptyList()
            },
        createdAt = createdAt,
    )

    fun importJob(
        status: JobStatus,
        step: String? = "extracting",
        done: Int? = 120,
        total: Int? = 456,
    ) = TopicJob(
        id = "j-doc",
        topicId = "t1",
        kind = "document_import",
        status = status,
        progress = if (step != null) JobProgress(step, null, done, total) else null,
        error = null,
        result = null,
        attempt = 1,
        createdAt = later,
        startedAt = later,
        finishedAt = null,
    )

    val noDocumentActions =
        DocumentsActions({}, {}, {}, {}, {}, {}, {}, {}, {})

    const val WHY = "Mantenere il backend di pagamenti scritto in Rust nel mio nuovo lavoro."
}
