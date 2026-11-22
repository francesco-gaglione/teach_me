package com.teachme.app

import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.requiresAppUpdate
import com.teachme.core.network.HealthApi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/** What "Ricontrolla" found out, for the update screen (llm-key.md section 10.2). */
enum class UpdateNotice { StillOutdated, Unreachable }

/**
 * The state of the re-check on the update screen: [checking] while the request is out, [notice] with the outcome and
 * [noticeId] (a new value for every outcome, so the same one is announced again) once it came back.
 */
data class UpdateCheckState(
    val checking: Boolean = false,
    val notice: UpdateNotice? = null,
    val noticeId: Int = 0,
)

/**
 * Compares the `api_version` of `GET /api/v1/health` with the major this build supports (api-conventions section 1).
 * [updateRequired] turns true when the server is ahead and the app must be updated: the host then blocks the UI.
 * Any other outcome (unreachable, signed out, odd answer) leaves the last known verdict alone: a flaky network must
 * neither lock the app nor unlock it. It never throws.
 */
@Singleton
class ApiCompatibility
    @Inject
    constructor(
        private val health: HealthApi,
    ) {
        private val _updateRequired = MutableStateFlow(false)
        val updateRequired: StateFlow<Boolean> = _updateRequired.asStateFlow()

        private val _recheck = MutableStateFlow(UpdateCheckState())
        val recheck: StateFlow<UpdateCheckState> = _recheck.asStateFlow()

        /** Bumped by [reset]: an answer that was asked for before it belongs to an earlier session and is dropped. */
        private var generation = 0

        suspend fun check() {
            val asked = generation
            val result = health.check()
            if (asked == generation && result is ConnectionResult.Connected) {
                _updateRequired.value = result.health.requiresAppUpdate()
            }
        }

        /** "Ricontrolla": like [check], but it reports its progress and outcome in [recheck]. */
        suspend fun recheckWithFeedback() {
            if (_recheck.value.checking) return
            val asked = generation
            _recheck.update { it.copy(checking = true, notice = null) }
            val result =
                try {
                    health.check()
                } catch (e: CancellationException) {
                    if (asked == generation) _recheck.update { it.copy(checking = false) }
                    throw e
                }
            // A reset (sign-out) happened while the request was out: its answer must not touch the new state.
            if (asked != generation) return
            val notice =
                when (result) {
                    is ConnectionResult.Connected -> {
                        val required = result.health.requiresAppUpdate()
                        _updateRequired.value = required
                        if (required) UpdateNotice.StillOutdated else null
                    }
                    // The session ended: the global handler takes the user to the login, nothing to say here.
                    ConnectionResult.SessionEnded -> null
                    is ConnectionResult.Unreachable, is ConnectionResult.Unexpected -> UpdateNotice.Unreachable
                }
            _recheck.update {
                UpdateCheckState(
                    checking = false,
                    notice = notice,
                    noticeId =
                        if (notice ==
                            null
                        ) {
                            it.noticeId
                        } else {
                            it.noticeId + 1
                        },
                )
            }
        }

        /** Sign-out: the verdict and the re-check outcome belong to the session, not to the next user. */
        fun reset() {
            generation++
            _updateRequired.value = false
            _recheck.value = UpdateCheckState()
        }
    }
