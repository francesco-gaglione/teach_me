package com.teachme.feature.settings

import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AuthState
import com.teachme.core.model.AuthUser
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.DueReviews
import com.teachme.core.model.Flash
import com.teachme.core.model.FlashInbox
import com.teachme.core.model.FlashKind
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyState
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoginOutcome
import com.teachme.core.model.NotificationPermission
import com.teachme.core.model.PendingCounts
import com.teachme.core.model.PendingSync
import com.teachme.core.model.QueuedReview
import com.teachme.core.model.ReviewCard
import com.teachme.core.model.ReviewCardKind
import com.teachme.core.model.ReviewCardStatus
import com.teachme.core.model.ReviewReminderPrefs
import com.teachme.core.model.ReviewReminderScheduler
import com.teachme.core.model.ReviewRepository
import com.teachme.core.model.ReviewState
import com.teachme.core.model.Session
import com.teachme.core.model.SessionActions
import com.teachme.core.model.SessionRefresher
import com.teachme.core.model.SessionRepository
import com.teachme.core.model.SuspendedReviews
import com.teachme.core.model.SuspensionReason
import com.teachme.core.model.UnreachableReason
import com.teachme.core.model.UserScope
import com.teachme.core.network.AuthApi
import com.teachme.core.network.HealthApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.time.Instant

internal val USER = AuthUser("u1", "valentina", "Valentina", hasLlmKey = true)

internal class FakeSessions(
    user: AuthUser = USER,
) : SessionRepository {
    private val _state =
        MutableStateFlow<AuthState>(AuthState.SignedIn(Session("https://teach.tailnet.ts.net", "tm_token", user)))
    override val state: StateFlow<AuthState> = _state
    val updates = mutableListOf<AuthUser>()

    override val scope: Flow<UserScope?> = _state.filter { it !is AuthState.Loading }.map { null }

    override suspend fun current(): UserScope? = null

    override suspend fun awaitReady(): AuthState = _state.first { it !is AuthState.Loading }

    override suspend fun signIn(
        baseUrl: String,
        token: String,
        user: AuthUser,
    ) = Unit

    override suspend fun signOut() = Unit

    override suspend fun expire(token: String) = false

    override suspend fun updateUser(user: AuthUser) {
        updates += user
        val session = (_state.value as AuthState.SignedIn).session
        _state.value = AuthState.SignedIn(session.copy(user = user))
    }
}

internal class FakeAuth : AuthApi {
    var setKey: suspend (String) -> ApiResult<AuthUser> = { ApiResult.Success(USER.copy(hasLlmKey = true)) }
    var removeKey: suspend () -> ApiResult<Unit> = { ApiResult.Success(Unit) }
    var password: suspend (String, String) -> ApiResult<Unit> = { _, _ -> ApiResult.Success(Unit) }
    var setVoice: suspend (String) -> ApiResult<AuthUser> = { ApiResult.Success(USER.copy(ttsVoice = it)) }
    val sentKeys = mutableListOf<String>()
    val sentVoices = mutableListOf<String>()
    val sentPasswords = mutableListOf<Pair<String, String>>()

    override suspend fun login(
        baseUrl: String,
        username: String,
        password: String,
        deviceName: String?,
    ): LoginOutcome = error("not used")

    override suspend fun logout(
        baseUrl: String,
        token: String,
    ) = true

    override suspend fun me(): ApiResult<AuthUser> = ApiResult.Failure(ApiFailure.Unauthorized)

    override suspend fun changePassword(
        currentPassword: String,
        newPassword: String,
    ): ApiResult<Unit> {
        sentPasswords += currentPassword to newPassword
        return password(currentPassword, newPassword)
    }

    override suspend fun setLlmKey(apiKey: String): ApiResult<AuthUser> {
        sentKeys += apiKey
        return setKey(apiKey)
    }

    override suspend fun removeLlmKey(): ApiResult<Unit> = removeKey()

    override suspend fun setTtsVoice(voice: String): ApiResult<AuthUser> {
        sentVoices += voice
        return setVoice(voice)
    }
}

internal class FakeHealth(
    var result: ConnectionResult,
) : HealthApi {
    override suspend fun check(override: com.teachme.core.model.BackendSettings?) = result
}

internal class FakeMonitor(
    initial: LlmKeyStatus = LlmKeyStatus(LlmKeyState.Set),
) : LlmKeyMonitor {
    override val status = MutableStateFlow(initial)
    val observed = mutableListOf<LlmKeyVariant>()
    var saved = 0
    var removed = 0

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: java.time.Instant,
    ) {
        observed += variant
    }

    override fun noteSaved(serverDate: java.time.Instant?) {
        saved++
        status.value = status.value.copy(state = LlmKeyState.Set, version = status.value.version + 1)
    }

    override fun noteWorked(at: java.time.Instant) = Unit

    override fun noteRemoved() {
        removed++
        status.value = status.value.copy(state = LlmKeyState.Missing, version = status.value.version + 1)
    }
}

internal class FakeRefresher : SessionRefresher {
    var refreshes = 0

    override suspend fun refresh() {
        refreshes++
    }

    override suspend fun refreshIfStale() = Unit
}

internal class FakePending(
    var queued: PendingCounts = PendingCounts(),
    var leftAfterFlush: PendingCounts = PendingCounts(),
) : PendingSync {
    var flushes = 0

    override suspend fun counts() = queued

    override suspend fun flush(timeoutMs: Long): PendingCounts {
        flushes++
        return leftAfterFlush
    }
}

internal class FakeActions : SessionActions {
    var signOuts = 0

    override suspend fun signOut() {
        signOuts++
    }
}

internal class FakeInbox : FlashInbox {
    val posted = mutableListOf<Flash>()

    override fun post(flash: Flash) {
        posted += flash
    }

    override fun take(vararg kinds: FlashKind): Flash? =
        posted.lastOrNull { it.kind in kinds }?.also { posted.remove(it) }
}

internal class FakeReminderPrefs(
    on: Boolean = true,
) : ReviewReminderPrefs {
    private val state = MutableStateFlow(on)
    var asked = false
    override val enabled: Flow<Boolean> = state

    override suspend fun isEnabled() = state.value

    override suspend fun setEnabled(enabled: Boolean) {
        state.value = enabled
    }

    override suspend fun permissionAsked() = asked

    override suspend fun markPermissionAsked() {
        asked = true
    }
}

internal class FakePermission(
    var granted: Boolean = true,
) : NotificationPermission {
    override fun isGranted() = granted
}

internal class FakeScheduler : ReviewReminderScheduler {
    val calls = mutableListOf<String>()

    override fun schedule() {
        calls += "schedule"
    }

    override fun cancel() {
        calls += "cancel"
    }
}

/** Scriptable [ReviewRepository] for the suspended-cards tests: pages are served in order, every call is recorded. */
internal class FakeReviews : ReviewRepository {
    /** Answers of `GET /reviews/suspended`, one per call; the last one repeats. */
    val pages = ArrayDeque<ApiResult<SuspendedReviews>>()
    val requests = mutableListOf<Pair<Int, String?>>()
    var pageGate: (suspend () -> Unit)? = null

    var unsuspendResult: (String) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it, suspended = false)) }
    var unsuspendGate: (suspend () -> Unit)? = null
    val unsuspended = mutableListOf<String>()

    var suspendResult: (String) -> ApiResult<ReviewCard> = { ApiResult.Success(card(it)) }
    val suspendedAgain = mutableListOf<String>()

    override suspend fun suspended(
        limit: Int,
        cursor: String?,
    ): ApiResult<SuspendedReviews> {
        requests += limit to cursor
        pageGate?.invoke()
        return if (pages.size > 1) pages.removeFirst() else pages.first()
    }

    override suspend fun unsuspend(cardId: String): ApiResult<ReviewCard> {
        unsuspended += cardId
        unsuspendGate?.invoke()
        return unsuspendResult(cardId)
    }

    override suspend fun suspend(cardId: String): ApiResult<ReviewCard> {
        suspendedAgain += cardId
        return suspendResult(cardId)
    }

    override suspend fun due(
        limit: Int,
        topicId: String?,
    ): ApiResult<DueReviews> = error("not used")

    override suspend fun rate(review: QueuedReview): ApiResult<ReviewCard> = error("not used")

    override suspend fun undo(cardId: String): ApiResult<ReviewCard> = error("not used")
}

internal fun card(
    id: String,
    suspended: Boolean = true,
    front: String = "Fronte $id",
    due: Instant? = null,
) = ReviewCard(
    id = id,
    topicId = "t",
    topicTitle = "Rust",
    lessonId = "l",
    lessonNumber = 2,
    lessonTitle = "Result",
    kind = ReviewCardKind.Record,
    front = front,
    back = "Retro $id",
    status = if (suspended) ReviewCardStatus.Suspended else ReviewCardStatus.Active,
    suspension = if (suspended) SuspensionReason.User else null,
    suspendedAt = if (suspended) Instant.parse("2026-11-15T09:00:00Z") else null,
    state = ReviewState.Review,
    due = due,
    lastReviewedAt = null,
    reviewCount = 1,
    intervals = null,
)

internal fun page(
    ids: List<String>,
    next: String? = null,
    count: Int = ids.size,
): ApiResult<SuspendedReviews> = ApiResult.Success(SuspendedReviews(ids.map { card(it) }, next, count))

internal val NETWORK_DOWN = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
