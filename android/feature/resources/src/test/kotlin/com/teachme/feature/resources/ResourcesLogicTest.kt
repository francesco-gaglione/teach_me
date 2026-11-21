package com.teachme.feature.resources

import com.teachme.core.model.JobError
import com.teachme.core.model.JobProgress
import com.teachme.core.model.JobStatus
import com.teachme.core.model.ResearchInfo
import com.teachme.core.model.ResearchStatus
import com.teachme.core.model.Resource
import com.teachme.core.model.ResourceKind
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.SourceType
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

private val T0 = Instant.parse("2026-11-09T14:00:00Z")

internal fun job(
    status: JobStatus,
    id: String = "j1",
    step: String? = null,
    message: String? = null,
    errorCode: String? = null,
    created: Instant = T0,
    started: Instant? = T0,
    resourceCount: Int? = null,
) = TopicJob(
    id = id,
    topicId = "t1",
    kind = "research",
    status = status,
    progress = if (step != null || message != null) JobProgress(step, message) else null,
    error = errorCode?.let { JobError(it, "raw $it") },
    result =
        resourceCount?.let {
            com.teachme.core.model
                .JobResult(it, 0)
        },
    attempt = 1,
    createdAt = created,
    startedAt = started,
    finishedAt = null,
)

internal fun source(
    id: String,
    url: String? = "https://example.org/$id",
    kind: ResourceKind = ResourceKind.Knowledge,
    domain: String? = "example.org",
) = Resource(id, kind, SourceType.Article, "Title $id", url, domain, "it", "annotation", "use", T0)

internal fun view(
    resources: List<Resource> = emptyList(),
    job: TopicJob? = null,
    research: ResearchInfo? = null,
    gaps: List<com.teachme.core.model.Gap> = emptyList(),
) = ResourcesView(resources, gaps, false, research, job)

class ResourcesLogicTest {
    @Test
    fun `phases follow the step and queued starts at the first`() {
        val running = phaseStates(job(JobStatus.Running, step = "reading"), completing = false).map { it.second }
        assertEquals(
            listOf(PhaseState.Done, PhaseState.Done, PhaseState.Current, PhaseState.Todo, PhaseState.Todo),
            running,
        )
        val queued = phaseStates(job(JobStatus.Queued), completing = false).map { it.second }
        assertEquals(PhaseState.Current, queued.first())
        assertTrue(queued.drop(1).all { it == PhaseState.Todo })
    }

    @Test
    fun `an unknown step highlights nothing and completing closes every phase`() {
        val unknown = phaseStates(job(JobStatus.Running, step = "from_the_future"), completing = false)
        assertTrue(unknown.all { it.second == PhaseState.Todo })
        val done = phaseStates(job(JobStatus.Succeeded), completing = true)
        assertTrue(done.all { it.second == PhaseState.Done })
    }

    @Test
    fun `slow after four minutes of an active job only`() {
        val running = job(JobStatus.Running, started = T0)
        assertFalse(isSlow(running, T0.plusSeconds(239)))
        assertTrue(isSlow(running, T0.plusSeconds(240)))
        assertFalse(isSlow(job(JobStatus.Failed, started = T0), T0.plusSeconds(1000)))
        // Not started yet: counted from the creation.
        assertTrue(isSlow(job(JobStatus.Queued, started = null), T0.plusSeconds(300)))
    }

    @Test
    fun `before the mission is confirmed the tab is empty`() {
        val empty = screenFor(TopicState.Interviewing, view(), null)
        assertEquals(ResourcesScreen.Empty, empty)
        assertEquals(ResourcesScreen.Empty, screenFor(TopicState.MissionProposed, view(), null))
    }

    @Test
    fun `researching with no job yet is a search in progress`() {
        val screen = screenFor(TopicState.Researching, view(), 1) as ResourcesScreen.Running
        assertNull(screen.job)
        assertNull(screen.previous)
    }

    @Test
    fun `a running job over an older list keeps the list below`() {
        val v = view(listOf(source("a")), job(JobStatus.Running, step = "searching"), ResearchInfo("old", 1, T0))
        val screen = screenFor(TopicState.Ready, v, 2) as ResourcesScreen.Running
        assertEquals(v, screen.previous)
        assertFalse(screen.completing)
    }

    @Test
    fun `a failed first search is the failed state, a failed update keeps the old list`() {
        val failed = view(job = job(JobStatus.Failed, errorCode = "timeout"))
        val first = screenFor(TopicState.Researching, failed, 1) as ResourcesScreen.Failed
        assertNull(first.previous)
        val refresh =
            screenFor(
                TopicState.Ready,
                failed.copy(resources = listOf(source("a")), research = ResearchInfo("old", 1, T0)),
                2,
            ) as ResourcesScreen.Failed
        assertEquals(1, refresh.previous?.resources?.size)
    }

    @Test
    fun `a succeeded job whose list is not loaded yet is completing, unless it gave up`() {
        val v = view(job = job(JobStatus.Succeeded), research = ResearchInfo("old-job", 1, T0))
        assertTrue((screenFor(TopicState.Ready, v, 1, settled = false) as ResourcesScreen.Running).completing)
        assertTrue(screenFor(TopicState.Ready, v, 1, settled = true) is ResourcesScreen.Results)
        // The list of that very job is there: done.
        val fresh = v.copy(research = ResearchInfo("j1", 1, T0), resources = listOf(source("a")))
        assertTrue(screenFor(TopicState.Ready, fresh, 1, settled = false) is ResourcesScreen.Results)
    }

    @Test
    fun `results are stale only when the mission is newer than the sources`() {
        val v = view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 2, T0))
        assertNull((screenFor(TopicState.Ready, v, 2) as ResourcesScreen.Results).staleVersion)
        assertNull((screenFor(TopicState.Ready, v, null) as ResourcesScreen.Results).staleVersion)
        assertEquals(2, (screenFor(TopicState.Ready, v, 3) as ResourcesScreen.Results).staleVersion)
    }

    @Test
    fun `finished without sources is still a result with no sources`() {
        val v = view(emptyList(), job(JobStatus.Succeeded), ResearchInfo("j1", 1, T0))
        val screen = screenFor(TopicState.Ready, v, 1) as ResourcesScreen.Results
        assertTrue(screen.view.resources.isEmpty())
    }

    @Test
    fun `error codes map to the four texts`() {
        assertEquals(FailureKind.Upstream, failureKind("upstream_failed"))
        assertEquals(FailureKind.RateLimited, failureKind("rate_limited"))
        assertEquals(FailureKind.Timeout, failureKind("timeout"))
        assertEquals(FailureKind.Generic, failureKind("interrupted"))
        assertEquals(FailureKind.Generic, failureKind("brand_new_code"))
        assertEquals(FailureKind.Generic, failureKind(null))
    }

    @Test
    fun `only https links are openable`() {
        assertEquals("https://a.org/x", source("a", url = " https://a.org/x ").openableUrl())
        assertNull(source("a", url = "http://a.org").openableUrl())
        assertNull(source("a", url = "javascript:alert(1)").openableUrl())
        assertNull(source("a", url = "https://").openableUrl())
        assertNull(source("a", url = null).openableUrl())
    }

    @Test
    fun `the shown domain is the real host without www`() {
        assertEquals("doc.rust-lang.org", source("a", domain = "doc.rust-lang.org").displayDomain())
        assertEquals("a.org", source("a", url = "https://www.a.org/x", domain = null).displayDomain())
        assertEquals("a.org", source("a", url = "https://a.org/x", domain = "www.a.org").displayDomain())
        assertNull(source("a", url = null, domain = null).displayDomain())
        assertNull(source("a", url = "not a url", domain = null).displayDomain())
    }

    @Test
    fun `new sources are compared by url and never on the first search`() {
        val old = listOf(source("a"), source("b"))
        val new = listOf(source("a"), source("c"), source("d", url = null))
        assertEquals(setOf("https://example.org/c"), newUrls(old, new))
        assertTrue(newUrls(emptyList(), new).isEmpty())
    }

    @Test
    fun `pointer for the other tabs`() {
        fun pointer(
            v: ResourcesView?,
            state: TopicState,
        ) = ResourcesUiState(view = v).toPointer(state)
        assertEquals(ResearchStatus.None, pointer(null, TopicState.Interviewing).status)
        assertEquals(ResearchStatus.InProgress, pointer(null, TopicState.Researching).status)
        assertEquals(
            ResearchStatus.InProgress,
            pointer(view(job = job(JobStatus.Running)), TopicState.Researching).status,
        )
        assertEquals(
            ResearchStatus.Failed,
            pointer(view(job = job(JobStatus.Failed)), TopicState.Researching).status,
        )
        val ready =
            pointer(view(listOf(source("a")), job(JobStatus.Succeeded), ResearchInfo("j1", 2, T0)), TopicState.Ready)
        assertEquals(ResearchStatus.Ready, ready.status)
        assertEquals(1, ready.resourceCount)
        assertTrue(ready.isStaleFor(3))
        assertFalse(ready.isStaleFor(2))
        // A successful search that found no sources is not a failure, even if a later update failed.
        val noSources =
            pointer(view(emptyList(), job(JobStatus.Failed), ResearchInfo("old", 1, T0)), TopicState.Ready)
        assertEquals(ResearchStatus.Ready, noSources.status)
        // A failed update over an old list is not "failed": the sources are still there.
        val failedUpdate =
            pointer(
                view(listOf(source("a")), job(JobStatus.Failed), ResearchInfo("old", 1, T0)),
                TopicState.Ready,
            )
        assertEquals(ResearchStatus.Ready, failedUpdate.status)
    }
}
