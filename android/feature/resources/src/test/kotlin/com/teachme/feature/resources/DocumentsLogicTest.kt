package com.teachme.feature.resources

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.DocumentError
import com.teachme.core.model.DocumentSection
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobStatus
import com.teachme.core.model.MAX_DOCUMENTS_PER_TOPIC
import com.teachme.core.model.MAX_DOCUMENT_BYTES
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicLanguage
import com.teachme.core.model.UnreachableReason
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.util.Locale

class DocumentsLogicTest {
    private val research = Instant.parse("2026-11-09T14:40:12Z")
    private val before = Instant.parse("2026-11-09T10:00:00Z")
    private val after = Instant.parse("2026-11-09T18:00:00Z")

    private fun doc(
        id: String = "d1",
        status: DocumentStatus = DocumentStatus.Ready,
        created: Instant? = after,
        errorCode: String? = null,
        title: String = "Rust in Action",
        filename: String = "rust-in-action.pdf",
    ) = TopicDocument(
        id,
        "t1",
        null,
        filename,
        title,
        status,
        errorCode?.let { DocumentError(it, "msg") },
        1_000L,
        10,
        "en",
        emptyList(),
        created,
    )

    private fun job(
        status: JobStatus,
        step: String? = null,
        done: Int? = null,
        total: Int? = null,
        code: String? = null,
        id: String = "j1",
        forDocument: String? = null,
    ) = TopicJob(
        id,
        "t1",
        "document_import",
        status,
        step?.let { JobProgress(it, null, done, total) },
        code?.let { JobError(it, "m") },
        null,
        1,
        after,
        null,
        null,
        documentId = forDocument,
    )

    // Size

    @Test
    fun `the 50 MB limit is exclusive and an unknown size passes`() {
        assertFalse(isOverSizeLimit(MAX_DOCUMENT_BYTES))
        assertTrue(isOverSizeLimit(MAX_DOCUMENT_BYTES + 1))
        assertFalse(isOverSizeLimit(0))
        assertFalse(isOverSizeLimit(null))
    }

    // Errors

    @Test
    fun `upload failures map to the panels of the spec`() {
        fun action(failure: ApiFailure) = uploadFailureAction(failure)
        assertEquals(UploadFailureAction.Show(UploadError.TooLarge), action(ApiFailure.Unexpected(413)))
        assertEquals(UploadFailureAction.Show(UploadError.NotPdf), action(ApiFailure.Unexpected(415)))
        assertEquals(UploadFailureAction.Show(UploadError.Network), action(ApiFailure.Unexpected(503)))
        assertEquals(UploadFailureAction.Show(UploadError.Network), action(ApiFailure.Unexpected(null)))
        assertEquals(
            UploadFailureAction.Show(UploadError.Network),
            action(ApiFailure.Network(UnreachableReason.Timeout)),
        )
        // The quota 409 gets its own panel (not the silent reload of a plain 409), and no retry: see UploadErrorPanel.
        assertEquals(UploadFailureAction.Show(UploadError.QuotaExceeded), action(ApiFailure.StorageQuotaExceeded))
        assertEquals(UploadFailureAction.Silent, action(ApiFailure.Unauthorized))
        assertEquals(UploadFailureAction.Reload, action(ApiFailure.Conflict))
        assertEquals(UploadFailureAction.TopicGone, action(ApiFailure.NotFound))
        assertEquals(UploadFailureAction.TopicGone, action(ApiFailure.InvalidId))
    }

    @Test
    fun `only the four file errors are final, anything else may be retried`() {
        assertEquals(DocumentFailure.NoText, DocumentFailure.fromCode("no_text"))
        assertEquals(DocumentFailure.Encrypted, DocumentFailure.fromCode("encrypted_pdf"))
        assertEquals(DocumentFailure.TooManyPages, DocumentFailure.fromCode("too_many_pages"))
        assertEquals(DocumentFailure.InvalidPdf, DocumentFailure.fromCode("invalid_pdf"))
        for (code in listOf("no_text", "encrypted_pdf", "too_many_pages", "invalid_pdf")) {
            assertFalse(code, DocumentFailure.fromCode(code).retryable)
        }
        for (code in listOf("timeout", "internal", "interrupted", "something_new", null)) {
            assertEquals(DocumentFailure.Other, DocumentFailure.fromCode(code))
            assertTrue(DocumentFailure.fromCode(code).retryable)
        }
    }

    @Test
    fun `retry is offered only for the newest failed document whose job failed with a retryable code`() {
        val failed = doc("d2", DocumentStatus.Failed, errorCode = "timeout", created = after)
        val older = doc("d1", DocumentStatus.Failed, errorCode = "timeout", created = before)
        val failedJob = job(JobStatus.Failed, code = "timeout")
        assertEquals(failedJob, retryableJob(failed, listOf(failed, older), failedJob))
        // The older one cannot be retried: the job is the latest one only.
        assertNull(retryableJob(older, listOf(failed, older), failedJob))
        // No job, or a job that did not fail: nothing to retry.
        assertNull(retryableJob(failed, listOf(failed), null))
        assertNull(retryableJob(failed, listOf(failed), job(JobStatus.Succeeded)))
        // A file error is never retried.
        val scan = doc("d3", DocumentStatus.Failed, errorCode = "no_text")
        assertNull(retryableJob(scan, listOf(scan), job(JobStatus.Failed, code = "no_text")))
        // A job that failed for another reason is another document's.
        assertNull(retryableJob(failed, listOf(failed), job(JobStatus.Failed, code = "internal")))
        // A ready document has nothing to retry.
        assertNull(retryableJob(doc(), listOf(doc()), failedJob))
    }

    @Test
    fun `a job that names its document is bound to it, otherwise the fallback applies`() {
        val a = doc("a", DocumentStatus.Failed, errorCode = "timeout", created = before)
        val b = doc("b", DocumentStatus.Failed, errorCode = "timeout", created = after)
        val named = job(JobStatus.Failed, code = "timeout", forDocument = "a")
        assertEquals(named, retryableJob(a, listOf(a, b), named))
        assertNull(retryableJob(b, listOf(a, b), named))
        assertEquals(named, jobFor(a, mapOf("a" to named), null))
        assertNull(jobFor(b, mapOf("a" to named), named))
        val plain = job(JobStatus.Failed, code = "timeout")
        assertEquals(plain, jobFor(b, emptyMap(), plain))
    }

    // Phases

    @Test
    fun `import phases follow the job step and have no percentage`() {
        assertEquals(ImportPhase.Queued, importPhase(job(JobStatus.Queued)))
        assertEquals(ImportPhase.Reading, importPhase(job(JobStatus.Running, "reading")))
        assertEquals(ImportPhase.Sectioning, importPhase(job(JobStatus.Running, "sectioning")))
        assertEquals(ImportPhase.Saving, importPhase(job(JobStatus.Running, "saving")))
        assertEquals(ImportPhase.Extracting(120, 456), importPhase(job(JobStatus.Running, "extracting", 120, 456)))
        assertEquals(ImportPhase.Extracting(null, null), importPhase(job(JobStatus.Running, "extracting")))
        assertEquals(ImportPhase.Unknown, importPhase(job(JobStatus.Running, "teleporting")))
        assertEquals(ImportPhase.Unknown, importPhase(job(JobStatus.Running)))
        assertEquals(ImportPhase.Unknown, importPhase(null))
        assertEquals(ImportPhase.Unknown, importPhase(job(JobStatus.Failed, "reading")))
    }

    @Test
    fun `a screen reader hears phase changes, never the page counter`() {
        val a = ImportPhase.Extracting(10, 456)
        val b = ImportPhase.Extracting(11, 456)
        assertEquals(a.announcedKey, b.announcedKey)
        assertFalse(ImportPhase.Reading.announcedKey == a.announcedKey)
        assertEquals(
            7,
            listOf(
                ImportPhase.Uploading,
                ImportPhase.Queued,
                ImportPhase.Reading,
                a,
                ImportPhase.Sectioning,
                ImportPhase.Saving,
                ImportPhase.Unknown,
            ).map {
                it.announcedKey
            }.toSet()
                .size,
        )
    }

    // Availability

    @Test
    fun `upload is blocked by the limit of 5 or by an import in progress`() {
        val five = List(MAX_DOCUMENTS_PER_TOPIC) { doc("d$it") }
        assertEquals(UploadAvailability.LimitReached, uploadAvailability(five, uploading = false))
        // A failed document counts toward the limit until it is removed.
        val failedFive = five.dropLast(1) + doc("x", DocumentStatus.Failed, errorCode = "no_text")
        assertEquals(UploadAvailability.LimitReached, uploadAvailability(failedFive, uploading = false))
        assertEquals(UploadAvailability.Available, uploadAvailability(five.take(4), uploading = false))
        assertEquals(UploadAvailability.ImportBusy, uploadAvailability(five.take(2), uploading = true))
        assertEquals(
            UploadAvailability.ImportBusy,
            uploadAvailability(listOf(doc("a", DocumentStatus.Importing)), uploading = false),
        )
        // A failed document does not block new uploads.
        assertEquals(
            UploadAvailability.Available,
            uploadAvailability(listOf(doc("a", DocumentStatus.Failed, errorCode = "no_text")), uploading = false),
        )
        assertEquals(UploadAvailability.Available, uploadAvailability(emptyList(), uploading = false))
    }

    // Banner "Ho letto il tuo PDF"

    private fun info(finished: Instant? = research) = ResearchInfo("job1", 2, finished)

    @Test
    fun `the banner names ready documents created after the research`() {
        val docs = listOf(doc("new", created = after), doc("old", created = before))
        val result = documentsNewerThanResearch(docs, info(), researchActive = false, dismissed = emptySet())
        assertEquals(listOf("new"), result.map { it.id })
    }

    @Test
    fun `no banner before a research, during one, or without a date to compare`() {
        val docs = listOf(doc("new"))
        assertTrue(documentsNewerThanResearch(docs, null, false, emptySet()).isEmpty())
        assertTrue(documentsNewerThanResearch(docs, info(), true, emptySet()).isEmpty())
        assertTrue(documentsNewerThanResearch(docs, info(finished = null), false, emptySet()).isEmpty())
    }

    @Test
    fun `no banner for documents still being read, failed, dismissed or with no date`() {
        val docs =
            listOf(
                doc("importing", DocumentStatus.Importing),
                doc("failed", DocumentStatus.Failed, errorCode = "no_text"),
                doc("dismissed"),
                doc("nodate", created = null),
            )
        assertTrue(documentsNewerThanResearch(docs, info(), false, setOf("dismissed")).isEmpty())
    }

    @Test
    fun `a document created at the very moment the research finished is not newer`() {
        val docs = listOf(doc("same", created = research))
        assertTrue(documentsNewerThanResearch(docs, info(), false, emptySet()).isEmpty())
    }

    @Test
    fun `removing a document makes its banner go away because the document is not in the list`() {
        val docs = listOf(doc("new"))
        assertEquals(1, documentsNewerThanResearch(docs, info(), false, emptySet()).size)
        assertTrue(documentsNewerThanResearch(emptyList(), info(), false, emptySet()).isEmpty())
    }

    // Presentation

    private fun section(
        title: String,
        start: Int = 1,
        end: Int = 24,
    ) = DocumentSection("s", title, 1, start, end)

    @Test
    fun `page ranges are hidden for the automatic blocks and singular for one page`() {
        assertEquals(PageRange(1, 24), section("1 Introducing Rust").pageRangeOrNull())
        assertTrue(section("1 Introducing Rust", 5, 5).pageRangeOrNull()!!.single)
        assertFalse(section("1 Introducing Rust").pageRangeOrNull()!!.single)
        assertNull(section("Pagine 1-10", 1, 10).pageRangeOrNull())
    }

    @Test
    fun `the index needs at least two sections`() {
        val one = doc().copy(sections = listOf(section("a")))
        val two = doc().copy(sections = listOf(section("a"), section("b")))
        assertFalse(doc().hasIndex())
        assertFalse(one.hasIndex())
        assertTrue(two.hasIndex())
    }

    @Test
    fun `the language is named only when it differs from the topic's and is known`() {
        val it = TopicLanguage.It
        assertEquals("inglese", languageNameOrNull("en", it, Locale.ITALIAN))
        assertEquals("francese", languageNameOrNull("fr-CA", it, Locale.ITALIAN))
        assertNull(languageNameOrNull("it", it, Locale.ITALIAN))
        assertNull(languageNameOrNull("it-IT", it, Locale.ITALIAN))
        assertNull(languageNameOrNull("en", TopicLanguage.En, Locale.ITALIAN))
        assertEquals("italiano", languageNameOrNull("it", TopicLanguage.En, Locale.ITALIAN))
        assertNull(languageNameOrNull(null, it, Locale.ITALIAN))
        assertNull(languageNameOrNull("", it, Locale.ITALIAN))
        assertNull(languageNameOrNull("zz", it, Locale.ITALIAN))
    }

    @Test
    fun `the file name is shown only when it adds something and the title is the file name while reading`() {
        assertFalse(doc(title = "rust-in-action", filename = "rust-in-action.pdf").showsFilename())
        assertTrue(doc(title = "Rust in Action", filename = "rust-in-action.pdf").showsFilename())
        assertEquals("Rust in Action", doc().listTitle())
        assertEquals("rust-in-action.pdf", doc(status = DocumentStatus.Importing).listTitle())
        assertEquals("rust-in-action.pdf", doc(status = DocumentStatus.Failed, errorCode = "no_text").listTitle())
    }
}
