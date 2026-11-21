package com.teachme.feature.resources

import com.teachme.core.model.JobStatus
import com.teachme.core.model.ResearchPointer
import com.teachme.core.model.ResearchStatus
import com.teachme.core.model.Resource
import com.teachme.core.model.ResourcesView
import com.teachme.core.model.TopicJob
import com.teachme.core.model.TopicState
import java.net.URI
import java.time.Duration
import java.time.Instant

/** After this long the progress panel admits it is taking longer than usual (resources.md 4.2). */
internal val SLOW_RESEARCH: Duration = Duration.ofMinutes(4)

/** Gaps shown before "Mostra le altre N" (resources.md section 5). */
internal const val GAPS_VISIBLE = 4

/** The five steps of the research job (resources.md section 4); the order is the order shown. */
internal enum class ResearchPhase(
    val step: String,
) {
    Planning("planning"),
    Searching("searching"),
    Reading("reading"),
    Evaluating("evaluating"),
    Saving("saving"),
    ;

    companion object {
        /** Null for a step this app does not know (a newer backend): no phase is highlighted. */
        fun fromStep(step: String?): ResearchPhase? = entries.firstOrNull { it.step == step }
    }
}

/** Where one phase stands in the panel. */
internal enum class PhaseState { Done, Current, Todo }

/**
 * The state of each phase for [job]. A finished job has every phase done; a queued or unknown-step job has the first
 * phase current (queued) or none (unknown), never a guess.
 */
internal fun phaseStates(
    job: TopicJob?,
    completing: Boolean,
): List<Pair<ResearchPhase, PhaseState>> {
    val phases = ResearchPhase.entries
    if (completing || job?.status == JobStatus.Succeeded) return phases.map { it to PhaseState.Done }
    val current =
        when {
            job == null || job.status == JobStatus.Queued -> ResearchPhase.Planning
            else -> ResearchPhase.fromStep(job.progress?.step)
        }
    if (current == null) return phases.map { it to PhaseState.Todo }
    return phases.map {
        it to
            when {
                it.ordinal < current.ordinal -> PhaseState.Done
                it == current -> PhaseState.Current
                else -> PhaseState.Todo
            }
    }
}

/** The job is slow when it has been going for [SLOW_RESEARCH] (counted from its start, else its creation). */
internal fun isSlow(
    job: TopicJob?,
    now: Instant,
): Boolean {
    if (job == null || !job.isActive) return false
    val since = job.startedAt ?: job.createdAt
    return Duration.between(since, now) >= SLOW_RESEARCH
}

/** What the Fonti tab shows (resources.md section 4). */
internal sealed interface ResourcesScreen {
    /** A: the mission is not confirmed yet. */
    data object Empty : ResourcesScreen

    /**
     * B (first search) and E (new search): [previous] is the older list, shown below the panel. [completing] is a job
     * that has just succeeded while the new list is still being loaded.
     */
    data class Running(
        val job: TopicJob?,
        val previous: ResourcesView?,
        val completing: Boolean = false,
    ) : ResourcesScreen

    /** D (and the failed E): [previous] is the older list that stays visible. */
    data class Failed(
        val job: TopicJob,
        val previous: ResourcesView?,
    ) : ResourcesScreen

    /** C and C0 (no sources): [staleVersion] is the version of the mission the sources were chosen for, if outdated. */
    data class Results(
        val view: ResourcesView,
        val staleVersion: Int?,
    ) : ResourcesScreen
}

internal fun ResourcesView.hasResults(): Boolean = research != null || resources.isNotEmpty() || gaps.isNotEmpty()

/**
 * The screen for the data. [settled] is false only while the list of a job that just succeeded is being fetched
 * again; if that fails for good the old list is shown instead of a panel that never ends.
 */
internal fun screenFor(
    topicState: TopicState,
    view: ResourcesView,
    currentMissionVersion: Int?,
    settled: Boolean = true,
): ResourcesScreen {
    val job = view.job
    val previous = view.takeIf { it.hasResults() }
    if (job != null && job.isActive) return ResourcesScreen.Running(job, previous)
    if (job != null && job.status == JobStatus.Failed) return ResourcesScreen.Failed(job, previous)
    if (job != null && job.status == JobStatus.Succeeded && view.research?.jobId != job.id && !settled) {
        return ResourcesScreen.Running(job, previous, completing = true)
    }
    if (view.hasResults()) {
        val version = view.research?.missionVersion
        val stale = version.takeIf { it != null && currentMissionVersion != null && it < currentMissionVersion }
        return ResourcesScreen.Results(view, stale)
    }
    return when (topicState) {
        // The Topic changed state before the first job event reached us: still a search in its first moments.
        TopicState.Researching -> ResourcesScreen.Running(null, null)
        TopicState.Ready -> ResourcesScreen.Results(view, null)
        else -> ResourcesScreen.Empty
    }
}

/** Why a failed research is shown the way it is (resources.md 4.4); `interrupted` and others read as generic. */
internal enum class FailureKind { Upstream, RateLimited, Timeout, Generic }

internal fun failureKind(code: String?): FailureKind =
    when (code) {
        "upstream_failed" -> FailureKind.Upstream
        "rate_limited" -> FailureKind.RateLimited
        "timeout" -> FailureKind.Timeout
        else -> FailureKind.Generic
    }

/** The pointer other tabs show, derived from the Topic state and the loaded data (resources.md section 7). */
fun ResourcesUiState.toPointer(topicState: TopicState): ResearchPointer {
    val view = view
    val job = view?.job
    return when {
        job != null && job.isActive ->
            ResearchPointer(
                ResearchStatus.InProgress,
                view.resources.size,
                view.research?.missionVersion,
            )
        job != null && job.status == JobStatus.Failed && view.resources.isEmpty() && view.research == null ->
            ResearchPointer(ResearchStatus.Failed)
        view != null && view.research != null ->
            ResearchPointer(ResearchStatus.Ready, view.readyCount(), view.research?.missionVersion)
        topicState == TopicState.Researching -> ResearchPointer(ResearchStatus.InProgress)
        else -> ResearchPointer.None
    }
}

/** The URL if the app may open it: `https` only (resources.md 8.3). */
internal fun Resource.openableUrl(): String? = url.httpsOrNull()

internal fun String?.httpsOrNull(): String? =
    this?.trim()?.takeIf { it.length > "https://".length && it.startsWith("https://", ignoreCase = true) }

/** The host the user sees before tapping: the backend's `domain`, else the real host of the URL, without `www.`. */
internal fun Resource.displayDomain(): String? {
    val fromUrl =
        url?.let { raw ->
            try {
                URI(raw.trim()).host
            } catch (_: Exception) {
                null
            }
        }
    return (domain ?: fromUrl)?.removePrefix("www.")?.takeIf { it.isNotBlank() }
}

/** URLs in [new] that were not in [old]; none on the very first search (nothing to compare with). */
internal fun newUrls(
    old: List<Resource>,
    new: List<Resource>,
): Set<String> {
    if (old.isEmpty()) return emptySet()
    val known = old.mapNotNull { it.url }.toSet()
    return new.mapNotNull { it.url }.filterNot { it in known }.toSet()
}

/** The count of the last finished research: the job's own result when it is that job, else the list length. */
private fun ResourcesView.readyCount(): Int =
    job?.takeIf { it.id == research?.jobId }?.result?.resourceCount ?: resources.size
