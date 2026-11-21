package com.teachme.core.model

import java.time.Instant

/** Lifecycle of one version of a mission (docs/contracts/mission.md section 2). */
enum class MissionStatus { Draft, Confirmed, Superseded }

/** One version of the mission of a Topic: why the user learns it and how they will know they succeeded. */
data class Mission(
    val topicId: String,
    val version: Int,
    val status: MissionStatus,
    val why: String,
    val successCriteria: List<String>,
    val constraints: List<String>,
    val outOfScope: List<String>,
    val createdAt: Instant,
    val confirmedAt: Instant?,
    val supersededAt: Instant?,
    /** What the person already knows (contract section 2, v1.2); null for versions saved before, never blank. */
    val startingPoint: String? = null,
)

/** `GET /topics/{id}/mission`: the confirmed mission and the open draft, either may be null. */
data class TopicMission(
    val current: Mission?,
    val draft: Mission?,
)

enum class PreferenceScope { Global, Topic }

/** A stable instruction about how to teach, saved by the Tutor (global or for one Topic). */
data class Preference(
    val id: String,
    val scope: PreferenceScope,
    val topicId: String?,
    val text: String,
    val createdAt: Instant,
)

/** Mission and preferences of a Topic (docs/contracts/mission.md). */
interface MissionRepository {
    suspend fun getMission(topicId: String): ApiResult<TopicMission>

    /** All versions, newest first. */
    suspend fun listVersions(topicId: String): ApiResult<List<Mission>>

    /** Confirms exactly [version]; a version that is not the open draft answers [ApiFailure.Conflict]. */
    suspend fun confirm(
        topicId: String,
        version: Int,
    ): ApiResult<TopicMission>

    /** Global preferences plus the ones of [topicId]. */
    suspend fun listPreferences(topicId: String): ApiResult<List<Preference>>

    suspend fun deletePreference(id: String): ApiResult<Unit>
}

/** Switches for features the backend does not offer yet. */
object FeatureFlags {
    /** Kept as a switch for a backend without the research. */
    @Suppress("ktlint:standard:property-naming")
    const val ResearchAvailable: Boolean = true
}

/** Max length of a preference text (contract section 2). */
const val PREFERENCE_TEXT_MAX = 300

/** A blank starting point is the same as none (mission.md contract section 8): it is never shown. */
fun String?.asStartingPoint(): String? = this?.takeIf { it.isNotBlank() }
