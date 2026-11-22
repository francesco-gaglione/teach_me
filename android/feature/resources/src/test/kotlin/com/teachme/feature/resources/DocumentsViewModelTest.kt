package com.teachme.feature.resources

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import app.cash.turbine.test
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.DocumentBannerStore
import com.teachme.core.model.DocumentError
import com.teachme.core.model.DocumentRepository
import com.teachme.core.model.DocumentStatus
import com.teachme.core.model.DocumentUpload
import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobResult
import com.teachme.core.model.JobStatus
import com.teachme.core.model.LoadError
import com.teachme.core.model.MAX_DOCUMENT_BYTES
import com.teachme.core.model.TopicDocument
import com.teachme.core.model.TopicJob
import com.teachme.core.model.UnreachableReason
import com.teachme.core.model.UploadSource
import com.teachme.core.model.UploadSourceRestorer
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class DocumentsViewModelTest {
    private val at = Instant.parse("2026-11-09T18:00:00Z")
    private val network = ApiFailure.Network(UnreachableReason.Timeout)

    private fun doc(
        id: String = "d1",
        status: DocumentStatus = DocumentStatus.Ready,
        errorCode: String? = null,
        created: Instant = at,
    ) = TopicDocument(
        id,
        "t1",
        null,
        "$id.pdf",
        "Titolo $id",
        status,
        errorCode?.let { DocumentError(it, "m") },
        1_000L,
        if (status == DocumentStatus.Ready) 456 else null,
        "en",
        emptyList(),
        created,
    )

    private fun job(
        status: JobStatus,
        step: String? = null,
        done: Int? = null,
        id: String = "j1",
        documentId: String? = null,
        code: String? = null,
        forDocument: String? = null,
        topicId: String = "t1",
        kind: String = "document_import",
        created: Instant = at,
    ) = TopicJob(
        id,
        topicId,
        kind,
        status,
        step?.let { JobProgress(it, null, done, 456) },
        code?.let { JobError(it, "m") },
        documentId?.let { JobResult(documentId = it, pageCount = 456, sectionCount = 3) },
        1,
        created,
        null,
        null,
        documentId = forDocument,
    )

    private class Source(
        override val displayName: String = "libro.pdf",
        override val sizeBytes: Long? = 100,
        override val reference: String? = "content://docs/libro",
        private val failOpening: Boolean = false,
    ) : UploadSource {
        override fun open(): InputStream =
            if (failOpening) throw SecurityException("permission denied") else ByteArrayInputStream(ByteArray(0))
    }

    /** The restorer of the app builds a source from the saved reference; here it records what it was asked for. */
    private class FakeRestorer(
        var result: ((String, String) -> UploadSource?) = { ref, name -> Source(name, null, ref) },
    ) : UploadSourceRestorer {
        val asked = mutableListOf<Pair<String, String>>()
        val released = mutableListOf<String>()

        override fun release(reference: String) {
            released += reference
        }

        override fun restore(
            reference: String,
            displayName: String,
        ): UploadSource? {
            asked += reference to displayName
            return result(reference, displayName)
        }
    }

    private class FakeDocuments : DocumentRepository {
        var documents: ApiResult<List<TopicDocument>> = ApiResult.Success(emptyList())
        var uploadResult: ApiResult<DocumentUpload>? = null
        var uploadGate: CompletableDeferred<Unit>? = null
        var throwOnUpload: Throwable? = null
        var latestJob: ApiResult<TopicJob?> = ApiResult.Success(null)
        var jobsList: ApiResult<List<TopicJob>>? = null
        var deleteResult: ApiResult<Unit> = ApiResult.Success(Unit)
        var retryResult: ApiResult<TopicJob> = ApiResult.Success(TopicJobs.queued())
        val uploaded = mutableListOf<UploadSource>()
        val deleted = mutableListOf<String>()
        val retried = mutableListOf<String>()
        var listCalls = 0
        val lists = ArrayDeque<ApiResult<List<TopicDocument>>>()

        override suspend fun list(topicId: String): ApiResult<List<TopicDocument>> {
            listCalls++
            return lists.removeFirstOrNull() ?: documents
        }

        override suspend fun upload(
            topicId: String,
            source: UploadSource,
        ): ApiResult<DocumentUpload> {
            uploaded += source
            uploadGate?.await()
            throwOnUpload?.let { throw it }
            return uploadResult!!
        }

        override suspend fun delete(documentId: String): ApiResult<Unit> {
            deleted += documentId
            return deleteResult
        }

        override suspend fun importJobs(topicId: String): ApiResult<List<TopicJob>> =
            jobsList ?: when (val r = latestJob) {
                is ApiResult.Success -> ApiResult.Success(listOfNotNull(r.value))
                is ApiResult.Failure -> r
            }

        override suspend fun retryJob(
            topicId: String,
            jobId: String,
        ): ApiResult<TopicJob> {
            retried += jobId
            return retryResult
        }
    }

    private object TopicJobs {
        fun queued() =
            TopicJob(
                "j1",
                "t1",
                "document_import",
                JobStatus.Queued,
                null,
                null,
                null,
                0,
                Instant.parse("2026-11-09T18:00:00Z"),
                null,
                null,
            )
    }

    private class FakeStore(
        var saved: Set<String> = emptySet(),
    ) : DocumentBannerStore {
        override suspend fun dismissed() = saved

        override suspend fun dismiss(documentIds: Collection<String>) {
            saved = saved + documentIds
        }
    }

    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.started(
        repo: FakeDocuments,
        store: FakeStore = FakeStore(),
        handle: SavedStateHandle = SavedStateHandle(),
        restorer: FakeRestorer = FakeRestorer(),
    ): DocumentsViewModel {
        val vm = DocumentsViewModel("t1", repo, store, restorer, handle)
        vm.clock = { dispatcher.scheduler.currentTime }
        vm.pollIntervalMs = Long.MAX_VALUE
        vm.onEvent(ChatEvent.ResyncRequired)
        advanceUntilIdle()
        return vm
    }

    private fun created(
        document: TopicDocument = doc("new", DocumentStatus.Importing),
        job: TopicJob = job(JobStatus.Queued, id = "j-new"),
    ) = ApiResult.Success(DocumentUpload(document, job))

    // Loading

    @Test
    fun `loads the documents and a failed first load shows the error until retried`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { documents = ApiResult.Success(listOf(doc("a"), doc("b"))) }
            val vm = started(repo)
            assertEquals(DocumentsLoad.Ready, vm.state.value.load)
            assertEquals(
                listOf("a", "b"),
                vm.state.value.documents
                    .map { it.id },
            )

            val failing = FakeDocuments().apply { documents = ApiResult.Failure(network) }
            val vm2 = started(failing)
            assertEquals(DocumentsLoad.Failed(LoadError.Network), vm2.state.value.load)
            failing.documents = ApiResult.Success(listOf(doc("a")))
            vm2.retryLoad()
            advanceUntilIdle()
            assertEquals(DocumentsLoad.Ready, vm2.state.value.load)
        }

    @Test
    fun `a failed refresh keeps the list on screen`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { documents = ApiResult.Success(listOf(doc("a"))) }
            val vm = started(repo)
            repo.documents = ApiResult.Failure(network)
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceUntilIdle()
            assertEquals(DocumentsLoad.Ready, vm.state.value.load)
            assertEquals(1, vm.state.value.documents.size)
        }

    @Test
    fun `a document still being read after a restart follows the latest import job`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Importing)))
                    latestJob = ApiResult.Success(job(JobStatus.Running, "extracting", 120))
                }
            val vm = started(repo)
            assertEquals(ImportPhase.Extracting(120, 456), importPhase(vm.state.value.importJob))
        }

    @Test
    fun `ready documents do not ask for the job`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a")))
                    latestJob = ApiResult.Failure(network)
                }
            val vm = started(repo)
            assertNull(vm.state.value.importJob)
        }

    // Upload

    @Test
    fun `a successful upload adds the importing document and follows its job`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a")))
                    uploadResult = created()
                }
            val vm = started(repo)
            val source = Source()
            vm.upload(source)
            assertEquals(UploadUi.Sending("libro.pdf"), vm.state.value.upload)
            advanceUntilIdle()
            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertEquals(
                listOf("new", "a"),
                vm.state.value.documents
                    .map { it.id },
            )
            assertEquals(
                "j-new",
                vm.state.value.importJob
                    ?.id,
            )
            assertSame(source, repo.uploaded.single())
        }

    @Test
    fun `a file over 50 MB is refused without a request`() =
        runTest(dispatcher) {
            val repo = FakeDocuments()
            val vm = started(repo)
            vm.upload(Source(sizeBytes = MAX_DOCUMENT_BYTES + 1))
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.TooLarge), vm.state.value.upload)
            assertTrue(repo.uploaded.isEmpty())
        }

    @Test
    fun `a file of exactly 50 MB or of unknown size is sent`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { uploadResult = created() }
            val vm = started(repo)
            vm.upload(Source(sizeBytes = MAX_DOCUMENT_BYTES))
            advanceUntilIdle()
            assertEquals(1, repo.uploaded.size)
            val repo2 = FakeDocuments().apply { uploadResult = created() }
            val vm2 = started(repo2)
            vm2.upload(Source(sizeBytes = null))
            advanceUntilIdle()
            assertEquals(1, repo2.uploaded.size)
        }

    @Test
    fun `a network error offers a retry that sends the same file again`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(network) }
            val vm = started(repo)
            val source = Source()
            vm.upload(source)
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.Network), vm.state.value.upload)
            assertTrue(
                vm.state.value.documents
                    .isEmpty(),
            )

            repo.uploadResult = created()
            vm.retryUpload()
            advanceUntilIdle()
            assertEquals(2, repo.uploaded.size)
            assertSame(source, repo.uploaded[1])
            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertEquals(1, vm.state.value.documents.size)
        }

    @Test
    fun `413 and 415 show their panels and are not retried with the same file`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(ApiFailure.Unexpected(413)) }
            val vm = started(repo)
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.TooLarge), vm.state.value.upload)

            repo.uploadResult = ApiResult.Failure(ApiFailure.Unexpected(415))
            vm.cancelUpload()
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.NotPdf), vm.state.value.upload)
        }

    @Test
    fun `409 reloads the list and shows no panel`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(ApiFailure.Conflict) }
            val vm = started(repo)
            val before = repo.listCalls
            repo.documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Importing)))
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertTrue(repo.listCalls > before)
            assertEquals(1, vm.state.value.documents.size)
        }

    @Test
    fun `404 on upload tells the screen the Topic is gone`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(ApiFailure.NotFound) }
            val vm = started(repo)
            vm.effects.test {
                vm.upload(Source())
                advanceUntilIdle()
                assertEquals(DocumentsEffect.TopicGone, awaitItem())
            }
        }

    @Test
    fun `cancel while sending drops the row and looks at the list again`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    uploadGate = CompletableDeferred()
                    uploadResult = created()
                }
            val vm = started(repo)
            vm.upload(Source())
            advanceUntilIdle()
            assertTrue(vm.state.value.upload is UploadUi.Sending)
            val before = repo.listCalls
            vm.cancelUpload()
            advanceUntilIdle()
            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertTrue(repo.listCalls > before)
            // The late answer of the cancelled request changes nothing.
            repo.uploadGate?.complete(Unit)
            advanceUntilIdle()
            assertTrue(
                vm.state.value.documents
                    .isEmpty(),
            )
        }

    @Test
    fun `no upload while a document is being read or at the limit of 5`() =
        runTest(dispatcher) {
            val busy =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(busy)
            vm.upload(Source())
            advanceUntilIdle()
            assertTrue(busy.uploaded.isEmpty())

            val full = FakeDocuments().apply { documents = ApiResult.Success(List(5) { doc("d$it") }) }
            val vm2 = started(full)
            vm2.upload(Source())
            advanceUntilIdle()
            assertTrue(full.uploaded.isEmpty())
        }

    // Job events

    @Test
    fun `job events move the phase and other kinds or topics are ignored`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "reading")))
            assertEquals(ImportPhase.Reading, importPhase(vm.state.value.importJob))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "saving", kind = "research")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "saving", topicId = "other")))
            assertEquals(ImportPhase.Reading, importPhase(vm.state.value.importJob))
        }

    @Test
    fun `page progress inside a step is applied at most once a second`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "extracting", 10)))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "extracting", 11)))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "extracting", 12)))
            assertEquals(ImportPhase.Extracting(10, 456), importPhase(vm.state.value.importJob))
            advanceTimeBy(PROGRESS_COALESCE_MS + 1)
            assertEquals(ImportPhase.Extracting(12, 456), importPhase(vm.state.value.importJob))
            // A new step is never held back.
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "sectioning")))
            assertEquals(ImportPhase.Sectioning, importPhase(vm.state.value.importJob))
        }

    @Test
    fun `a succeeded import reloads the list, announces it and tells the screen`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "saving")))
            repo.documents = ApiResult.Success(listOf(doc("a")))
            vm.effects.test {
                vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, documentId = "a")))
                advanceUntilIdle()
                assertEquals(DocumentsEffect.Imported("Titolo a", 456), awaitItem())
            }
            assertEquals(
                DocumentStatus.Ready,
                vm.state.value.documents
                    .single()
                    .status,
            )
            val announcement = vm.state.value.announcement as DocumentAnnouncement.Read
            assertEquals("Titolo a", announcement.title)
            assertEquals(456, announcement.pages)
        }

    @Test
    fun `the same finished job is handled only once`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "saving")))
            repo.documents = ApiResult.Success(listOf(doc("a")))
            vm.effects.test {
                vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, documentId = "a")))
                advanceUntilIdle()
                awaitItem()
                vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, documentId = "a")))
                advanceUntilIdle()
                expectNoEvents()
            }
        }

    @Test
    fun `a failed import shows the document with its error code and announces it`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "extracting", 3)))
            repo.documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Failed, errorCode = "no_text")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Failed, code = "no_text")))
            advanceUntilIdle()
            val document =
                vm.state.value.documents
                    .single()
            assertEquals(DocumentStatus.Failed, document.status)
            assertEquals("no_text", document.error?.code)
            val announcement = vm.state.value.announcement as DocumentAnnouncement.Failed
            assertEquals(DocumentFailure.NoText, announcement.failure)
        }

    @Test
    fun `the list is read again until the server shows the end of the import`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = started(repo)
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Running, "saving")))
            // The first read still shows the old state, the second the new one.
            repo.lists += ApiResult.Success(listOf(doc("a", DocumentStatus.Importing)))
            repo.lists += ApiResult.Success(listOf(doc("a")))
            vm.onEvent(ChatEvent.JobUpdated(job(JobStatus.Succeeded, documentId = "a")))
            advanceUntilIdle()
            assertEquals(
                DocumentStatus.Ready,
                vm.state.value.documents
                    .single()
                    .status,
            )
        }

    // Retry

    @Test
    fun `retry posts the failed job of the newest document and reloads`() =
        runTest(dispatcher) {
            val failed = doc("a", DocumentStatus.Failed, errorCode = "timeout")
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(failed))
                    latestJob = ApiResult.Success(job(JobStatus.Failed, id = "jf", code = "timeout"))
                    retryResult = ApiResult.Success(job(JobStatus.Queued, id = "jf"))
                }
            val vm = started(repo)
            // After the retry the server lists the same job as queued again, and the document as importing.
            repo.documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Importing)))
            repo.latestJob = ApiResult.Success(job(JobStatus.Queued, id = "jf"))
            vm.retryImport("a")
            advanceUntilIdle()
            assertEquals(listOf("jf"), repo.retried)
            assertEquals(
                JobStatus.Queued,
                vm.state.value.importJob
                    ?.status,
            )
            assertEquals(
                DocumentStatus.Importing,
                vm.state.value.documents
                    .single()
                    .status,
            )
            assertFalse(vm.state.value.retrying)
        }

    @Test
    fun `retry is ignored for a document whose error is the files`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Failed, errorCode = "no_text")))
                    latestJob = ApiResult.Success(job(JobStatus.Failed, code = "no_text"))
                }
            val vm = started(repo)
            vm.retryImport("a")
            advanceUntilIdle()
            assertTrue(repo.retried.isEmpty())
        }

    @Test
    fun `a failed retry request says so`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a", DocumentStatus.Failed, errorCode = "timeout")))
                    latestJob = ApiResult.Success(job(JobStatus.Failed, code = "timeout"))
                    retryResult = ApiResult.Failure(network)
                }
            val vm = started(repo)
            vm.effects.test {
                vm.retryImport("a")
                advanceUntilIdle()
                assertEquals(DocumentsEffect.RetryFailed, awaitItem())
            }
        }

    // Delete

    @Test
    fun `delete removes the row and tells the screen`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { documents = ApiResult.Success(listOf(doc("a"), doc("b"))) }
            val vm = started(repo)
            vm.effects.test {
                vm.delete("a")
                advanceUntilIdle()
                assertEquals(DocumentsEffect.Removed, awaitItem())
            }
            assertEquals(
                listOf("b"),
                vm.state.value.documents
                    .map { it.id },
            )
            assertEquals(DeleteUi.Idle, vm.state.value.delete)
        }

    @Test
    fun `the quota panel has no retry, closes on dismiss, and goes away when a document is deleted`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a")))
                    uploadResult = ApiResult.Failure(ApiFailure.StorageQuotaExceeded)
                }
            val vm = started(repo)
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.QuotaExceeded), vm.state.value.upload)

            // Dismiss with the X: back to idle, nothing sent again.
            vm.cancelUpload()
            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertEquals(1, repo.uploaded.size)

            // Shown again; deleting a document makes it disappear by itself.
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.QuotaExceeded), vm.state.value.upload)
            vm.delete("a")
            advanceUntilIdle()
            assertEquals(UploadUi.Idle, vm.state.value.upload)
        }

    @Test
    fun `choosing a new file replaces the quota panel, and other upload errors survive a deletion`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a")))
                    uploadResult = ApiResult.Failure(ApiFailure.StorageQuotaExceeded)
                }
            val vm = started(repo)
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.QuotaExceeded), vm.state.value.upload)

            repo.uploadResult = ApiResult.Failure(ApiFailure.Unexpected(415))
            vm.upload(Source())
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.NotPdf), vm.state.value.upload)

            // Only the quota panel is tied to the free space: a wrong file type stays until closed.
            vm.delete("a")
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.NotPdf), vm.state.value.upload)
        }

    @Test
    fun `delete errors keep the row and say what happened`() =
        runTest(dispatcher) {
            val repo = FakeDocuments().apply { documents = ApiResult.Success(listOf(doc("a"))) }
            val vm = started(repo)
            repo.deleteResult = ApiResult.Failure(ApiFailure.Conflict)
            vm.delete("a")
            advanceUntilIdle()
            assertEquals(DeleteUi.Failed("a", DeleteError.Busy), vm.state.value.delete)
            assertEquals(1, vm.state.value.documents.size)

            vm.clearDeleteError()
            repo.deleteResult = ApiResult.Failure(network)
            vm.delete("a")
            advanceUntilIdle()
            assertEquals(DeleteUi.Failed("a", DeleteError.Network), vm.state.value.delete)

            // Already gone on the server: the goal is reached.
            repo.deleteResult = ApiResult.Failure(ApiFailure.NotFound)
            vm.delete("a")
            advanceUntilIdle()
            assertTrue(
                vm.state.value.documents
                    .isEmpty(),
            )
        }

    // Banner

    @Test
    fun `not now is kept on the device and read back by the next screen`() =
        runTest(dispatcher) {
            val store = FakeStore()
            val vm = started(FakeDocuments(), store)
            vm.dismissBanner(listOf("a", "b"))
            advanceUntilIdle()
            assertEquals(setOf("a", "b"), vm.state.value.dismissedBanner)
            assertEquals(setOf("a", "b"), store.saved)

            val next = started(FakeDocuments(), store)
            assertEquals(setOf("a", "b"), next.state.value.dismissedBanner)
        }

    @Test
    fun `the poll reads the list again while a document is being read`() =
        runTest(dispatcher) {
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(
                            listOf(doc("a", DocumentStatus.Importing)),
                        )
                }
            val vm = DocumentsViewModel("t1", repo, FakeStore(), FakeRestorer(), SavedStateHandle())
            vm.pollIntervalMs = 15_000
            vm.onEvent(ChatEvent.ResyncRequired)
            advanceTimeBy(1)
            val first = repo.listCalls
            repo.documents = ApiResult.Success(listOf(doc("a")))
            advanceTimeBy(15_001)
            assertTrue(repo.listCalls > first)
            assertEquals(
                DocumentStatus.Ready,
                vm.state.value.documents
                    .single()
                    .status,
            )
        }

    @Test
    fun `progress and retry are bound to the document the job names`() =
        runTest(dispatcher) {
            val old =
                doc(
                    "old",
                    DocumentStatus.Failed,
                    errorCode = "timeout",
                    created = Instant.parse("2026-11-09T10:00:00Z"),
                )
            val reading = doc("new", DocumentStatus.Importing)
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(reading, old))
                    jobsList =
                        ApiResult.Success(
                            listOf(
                                job(JobStatus.Running, "saving", id = "j2", forDocument = "new"),
                                job(JobStatus.Failed, id = "j1", code = "timeout", forDocument = "old"),
                            ),
                        )
                    retryResult = ApiResult.Success(job(JobStatus.Queued, id = "j1", forDocument = "old"))
                }
            val vm = started(repo)
            val state = vm.state.value
            assertEquals(ImportPhase.Saving, importPhase(jobFor(reading, state.jobs, state.importJob)))
            // The older failed document is retryable although it is not the newest one.
            assertEquals("j1", retryableJob(old, state.documents, jobFor(old, state.jobs, state.importJob))?.id)
            vm.retryImport("old")
            advanceUntilIdle()
            assertEquals(listOf("j1"), repo.retried)
        }

    @Test
    fun `an old ended job next to an import in progress does not reload the list forever`() =
        runTest(dispatcher) {
            val old = doc("old", DocumentStatus.Failed, errorCode = "timeout")
            val reading = doc("new", DocumentStatus.Importing)
            // Same creation time on purpose, with and without document_id (older backend): the cases that looped.
            listOf("old", null).forEach { named ->
                val repo =
                    FakeDocuments().apply {
                        documents = ApiResult.Success(listOf(reading, old))
                        jobsList =
                            ApiResult.Success(
                                listOf(
                                    job(JobStatus.Running, "extracting", id = "j2", forDocument = "new"),
                                    job(JobStatus.Failed, id = "j1", code = "timeout", forDocument = named),
                                ),
                            )
                    }
                val vm = DocumentsViewModel("t1", repo, FakeStore(), FakeRestorer(), SavedStateHandle())
                vm.clock = { dispatcher.scheduler.currentTime }
                vm.pollIntervalMs = Long.MAX_VALUE
                vm.onEvent(ChatEvent.ResyncRequired)
                try {
                    // Bounded virtual time: advanceUntilIdle would never return if the reloads looped.
                    advanceTimeBy(60_000)
                    assertTrue("list read ${repo.listCalls} times (document_id=$named)", repo.listCalls <= 5)
                } finally {
                    // Stop the loop if there is one, or the end of runTest would wait for it forever.
                    vm.viewModelScope.cancel()
                }
            }
        }

    @Test
    fun `after process death a failed upload is shown with retry and retry sends the same source again`() =
        runTest(dispatcher) {
            val handle = SavedStateHandle()
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(network) }
            val first = started(repo, handle = handle)
            first.upload(Source("tesi.pdf", reference = "content://docs/tesi"))
            advanceUntilIdle()
            assertEquals(UploadUi.Failed(UploadError.Network), first.state.value.upload)
            first.viewModelScope.cancel()

            // New ViewModel, same handle: the failed state comes back, no file bytes were kept.
            val restorer = FakeRestorer()
            val restored = started(repo, handle = handle, restorer = restorer)
            assertEquals(UploadUi.Failed(UploadError.Network), restored.state.value.upload)

            repo.uploadResult = created()
            restored.retryUpload()
            advanceUntilIdle()
            assertEquals(listOf("content://docs/tesi" to "tesi.pdf"), restorer.asked)
            assertEquals("tesi.pdf", repo.uploaded.last().displayName)
            assertEquals("content://docs/tesi", repo.uploaded.last().reference)
            assertEquals(UploadUi.Idle, restored.state.value.upload)
            assertNull(handle.get<String>("upload_reference"))
        }

    @Test
    fun `an upload interrupted by process death comes back as a failed upload`() =
        runTest(dispatcher) {
            val handle = SavedStateHandle()
            val repo = FakeDocuments().apply { uploadGate = CompletableDeferred() }
            val first = started(repo, handle = handle)
            first.upload(Source("tesi.pdf"))
            advanceUntilIdle()
            assertTrue(first.state.value.upload is UploadUi.Sending)
            first.viewModelScope.cancel()

            val restored = started(FakeDocuments(), handle = handle)
            assertEquals(UploadUi.Failed(UploadError.Network), restored.state.value.upload)
        }

    @Test
    fun `a restored source whose uri grant was not persisted fails with a SecurityException as a normal failure`() =
        runTest(dispatcher) {
            // Limit: a content:// URI without a persisted permission is unreadable after a restart.
            val handle = SavedStateHandle(mapOf("upload_reference" to "content://docs/x", "upload_name" to "x.pdf"))
            val repo = FakeDocuments()
            val restored =
                started(
                    repo,
                    handle = handle,
                    restorer = FakeRestorer { ref, name -> Source(name, null, ref, failOpening = true) },
                )
            repo.throwOnUpload = SecurityException("permission denied")

            restored.retryUpload()
            advanceUntilIdle()

            assertEquals(UploadUi.Failed(UploadError.Network), restored.state.value.upload)
            assertTrue(
                restored.state.value.documents
                    .isEmpty(),
            )
            // "Chiudi" ends it and nothing stays saved.
            restored.cancelUpload()
            assertEquals(UploadUi.Idle, restored.state.value.upload)
            assertNull(handle.get<String>("upload_reference"))
        }

    @Test
    fun `retry after process death with a source that cannot be rebuilt goes back to idle`() =
        runTest(dispatcher) {
            val handle = SavedStateHandle(mapOf("upload_reference" to "content://docs/x", "upload_name" to "x.pdf"))
            val repo = FakeDocuments()
            val restored = started(repo, handle = handle, restorer = FakeRestorer { _, _ -> null })
            restored.retryUpload()
            advanceUntilIdle()
            assertEquals(UploadUi.Idle, restored.state.value.upload)
            assertTrue(repo.uploaded.isEmpty())
            assertNull(handle.get<String>("upload_reference"))
        }

    @Test
    fun `a source without a reference or a refused file is not saved`() =
        runTest(dispatcher) {
            val handle = SavedStateHandle()
            val vm = started(FakeDocuments().apply { uploadResult = ApiResult.Failure(network) }, handle = handle)
            vm.upload(Source("a.pdf", reference = null))
            advanceUntilIdle()
            assertNull(handle.get<String>("upload_reference"))
            vm.cancelUpload()
            vm.upload(Source("big.pdf", sizeBytes = MAX_DOCUMENT_BYTES + 1))
            assertEquals(UploadUi.Failed(UploadError.TooLarge), vm.state.value.upload)
            assertNull(handle.get<String>("upload_reference"))
        }

    @Test
    fun `a cleared ViewModel stops talking to the backend and the next one starts empty`() =
        runTest(dispatcher) {
            val repoA =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("a-doc")))
                    uploadResult = ApiResult.Failure(network)
                }
            val storeA = FakeStore(setOf("a-doc"))
            val handleA = SavedStateHandle()
            val a = started(repoA, storeA, handleA)
            a.upload(Source("segreto-di-A.pdf"))
            advanceUntilIdle()
            assertEquals(1, a.state.value.documents.size)
            assertEquals(setOf("a-doc"), a.state.value.dismissedBanner)
            assertTrue(a.state.value.upload is UploadUi.Failed)

            // Sign-out pops the stack: A's ViewModel goes, B's starts with its own handle, repository and store.
            a.viewModelScope.cancel()
            val repoB = FakeDocuments().apply { documents = ApiResult.Success(listOf(doc("b-doc"))) }
            val callsOfA = repoA.listCalls
            val b = started(repoB, FakeStore(), SavedStateHandle())
            advanceTimeBy(60_000)
            assertEquals(callsOfA, repoA.listCalls)

            assertEquals(
                listOf("b-doc"),
                b.state.value.documents
                    .map { it.id },
            )
            assertTrue(
                b.state.value.dismissedBanner
                    .isEmpty(),
            )
            assertEquals(UploadUi.Idle, b.state.value.upload)
            assertNull(b.state.value.importJob)
            b.retryUpload()
            advanceUntilIdle()
            assertTrue(repoB.uploaded.isEmpty())
        }

    @Test
    fun `after process death a document with the same name created after the upload started means it is done`() =
        runTest(dispatcher) {
            val started = at.toEpochMilli()
            val handle =
                SavedStateHandle(
                    mapOf(
                        "upload_reference" to "content://docs/t",
                        "upload_name" to "tesi.pdf",
                        "upload_started_at" to started,
                    ),
                )
            val repo =
                FakeDocuments().apply {
                    documents =
                        ApiResult.Success(listOf(doc("tesi", DocumentStatus.Importing, created = at.plusSeconds(5))))
                }
            val restorer = FakeRestorer()
            val vm = started(repo, handle = handle, restorer = restorer)

            assertEquals(UploadUi.Idle, vm.state.value.upload)
            assertNull(handle.get<String>("upload_reference"))
            assertEquals(listOf("content://docs/t"), restorer.released)
            vm.retryUpload()
            advanceUntilIdle()
            assertTrue(repo.uploaded.isEmpty())
        }

    @Test
    fun `after process death an older document with the same name does not hide the retry`() =
        runTest(dispatcher) {
            val handle =
                SavedStateHandle(
                    mapOf(
                        "upload_reference" to "content://docs/t",
                        "upload_name" to "tesi.pdf",
                        "upload_started_at" to at.toEpochMilli(),
                    ),
                )
            val repo =
                FakeDocuments().apply {
                    documents = ApiResult.Success(listOf(doc("tesi", created = at.minusSeconds(3_600))))
                }
            val vm = started(repo, handle = handle)
            assertEquals(UploadUi.Failed(UploadError.Network), vm.state.value.upload)
            assertEquals("content://docs/t", handle.get<String>("upload_reference"))
        }

    @Test
    fun `after process death a list that cannot be read still offers the retry`() =
        runTest(dispatcher) {
            val handle =
                SavedStateHandle(mapOf("upload_reference" to "content://docs/t", "upload_name" to "tesi.pdf"))
            val repo = FakeDocuments().apply { documents = ApiResult.Failure(network) }
            val vm = started(repo, handle = handle)
            assertEquals(UploadUi.Failed(UploadError.Network), vm.state.value.upload)
        }

    @Test
    fun `forgetting the saved source releases its persisted permission`() =
        runTest(dispatcher) {
            val restorer = FakeRestorer()
            val repo = FakeDocuments().apply { uploadResult = ApiResult.Failure(network) }
            val vm = started(repo, restorer = restorer)
            vm.upload(Source("a.pdf", reference = "content://docs/a"))
            advanceUntilIdle()
            vm.cancelUpload()
            assertEquals(listOf("content://docs/a"), restorer.released)
        }
}
