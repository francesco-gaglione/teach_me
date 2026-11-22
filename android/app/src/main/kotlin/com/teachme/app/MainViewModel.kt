package com.teachme.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.teachme.core.model.AuthState
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.ReviewReminderGate
import com.teachme.core.model.ReviewSyncer
import com.teachme.core.model.SessionRefresher
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.clearFlashesOnSessionChange
import com.teachme.core.model.signInTransitions
import com.teachme.feature.lesson.AttemptSyncer
import com.teachme.feature.lesson.HighlightSyncer
import com.teachme.feature.lesson.StrokeSyncer
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Holds the session for the navigation host, and keeps the user's data moving while the app is open. */
@HiltViewModel
class MainViewModel
    @Inject
    constructor(
        private val sessions: SessionRepository,
        private val syncer: AttemptSyncer,
        private val highlightSyncer: HighlightSyncer,
        private val strokeSyncer: StrokeSyncer,
        private val reviewSyncer: ReviewSyncer,
        private val refresher: SessionRefresher,
        private val llmKey: LlmKeyMonitor,
        private val reminderGate: ReviewReminderGate,
        private val flashInbox: FlashInbox,
        private val compatibility: ApiCompatibility,
    ) : ViewModel() {
        /** [AuthState.Loading] until the stored session has been read; the first frame waits for it. */
        val authState: StateFlow<AuthState> = sessions.state

        /** True when the server's API is newer than this app understands: the UI is blocked by "Aggiorna l'app". */
        val updateRequired: StateFlow<Boolean> = compatibility.updateRequired

        /** Progress and outcome of "Ricontrolla" on the update screen. */
        val updateCheck: StateFlow<UpdateCheckState> = compatibility.recheck

        /** The known state of the user's key, for every notice and hint (llm-key.md). */
        val llmKeyStatus: StateFlow<LlmKeyStatus> = llmKey.status

        init {
            // A message left for the next screen (it may name a folder or topic) never outlives the session that posted it.
            viewModelScope.launch { sessions.state.clearFlashesOnSessionChange(flashInbox) }
            // The hourly check follows the session (ADR 0012 point 7): it runs for a signed-in user with the device switch
            // on and the permission granted; sign-out and an expired session (`401`) cancel it and the notification.
            viewModelScope.launch { sessions.state.collect { reminderGate.onSession(it) } }
            // Signing out clears the "update the app" verdict: it is re-read when the next user signs in.
            viewModelScope.launch {
                sessions.state.collect {
                    if (it is AuthState.SignedOut) {
                        compatibility.reset()
                        // The login screen must not talk to a newer API either: read the version again, without a session.
                        launch { compatibility.check() }
                    }
                }
            }
            // Whenever a user signs in (the app starts with a session, or the login succeeds): send what is queued for
            // them and ask `GET /me`. Queues of another user are never touched: they are keyed by the signed-in user.
            viewModelScope.launch {
                sessions.state
                    .signInTransitions()
                    .collect {
                        launch { compatibility.check() }
                        launch { refresher.refresh() }
                        drainQueues()
                    }
            }
        }

        /** Back in the foreground: queued answers and grades go out, and `GET /me` at most once a minute. */
        fun onForeground() {
            // `/health` is public: the compatibility check runs with or without a session, so the login screen is covered too.
            viewModelScope.launch { compatibility.check() }
            viewModelScope.launch {
                if (sessions.state.value !is AuthState.SignedIn) return@launch
                launch { refresher.refreshIfStale() }
                // The permission or the notification settings may have changed while the app was away.
                launch { reminderGate.sync() }
                drainQueues()
            }
        }

        /** "Ricontrolla" on the update screen. */
        fun recheckCompatibility() {
            viewModelScope.launch { compatibility.recheckWithFeedback() }
        }

        private suspend fun drainQueues() {
            viewModelScope.launch { syncer.flushAll() }
            // The highlights that did not get out before the lesson was closed.
            viewModelScope.launch { highlightSyncer.flushAll() }
            // The handwritten notes that did not get out before the lesson was closed.
            viewModelScope.launch { strokeSyncer.flushAll() }
            // The review grades that did not get out before the app was closed or lost the network.
            viewModelScope.launch { reviewSyncer.flush() }
        }
    }
