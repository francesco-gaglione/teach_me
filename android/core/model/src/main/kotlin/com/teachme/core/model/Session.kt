package com.teachme.core.model

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

/** The LLM service the server is configured with (users.md section 1). Unknown values are tolerated. */
enum class LlmProvider {
    Anthropic,
    OpenRouter,
    Unknown,
    ;

    companion object {
        fun fromApi(value: String?): LlmProvider =
            when (value) {
                "anthropic" -> Anthropic
                "openrouter" -> OpenRouter
                else -> Unknown
            }
    }
}

/** `User` of users.md section 1. The LLM key itself never reaches the app: only whether one exists. */
data class AuthUser(
    val id: String,
    val username: String,
    val displayName: String,
    val hasLlmKey: Boolean,
    val llmProvider: LlmProvider = LlmProvider.Unknown,
    /** The effective read-aloud voice id (audio.md section 4.2); null when the feature is off or not known yet. */
    val ttsVoice: String? = null,
)

/** Who used this device last: kept after the session ends so the login screen can be prefilled. */
data class RememberedUser(
    val id: String,
    val username: String,
    val displayName: String,
)

/**
 * Whose local data this is: one user on one backend. Every on-device store that holds user data (queues of
 * answers and grades) is keyed by [key], so data of a user never leaves with the session of another.
 */
class UserScope(
    val userId: String,
    baseUrl: String,
) {
    /** The address as the backend is reached: trailing slash and `/api/v1` removed, scheme in lowercase. */
    val baseUrl: String = baseUrl.normalizedBaseUrl()

    val key: String get() = "$userId@$baseUrl"

    override fun equals(other: Any?): Boolean = other is UserScope && other.key == key

    override fun hashCode(): Int = key.hashCode()

    override fun toString(): String = "UserScope($key)"
}

/** A live session. [token] is a secret: it never appears in [toString], logs or crash reports. */
class Session(
    val baseUrl: String,
    val token: String,
    val user: AuthUser,
) {
    val scope: UserScope get() = UserScope(user.id, baseUrl)

    fun copy(user: AuthUser = this.user) = Session(baseUrl, token, user)

    override fun toString(): String = "Session(baseUrl=$baseUrl, user=${user.id})"

    override fun equals(other: Any?): Boolean =
        other is Session && other.baseUrl == baseUrl && other.token == token && other.user == user

    override fun hashCode(): Int = 31 * (31 * baseUrl.hashCode() + token.hashCode()) + user.hashCode()
}

/** Why there is no session (login.md, the four variants). */
enum class SignedOutReason {
    /** No address saved yet. */
    FirstRun,

    /** The app was updated from the single-user version: address kept, static token discarded. */
    Upgrade,

    /** The user chose "Esci". */
    LoggedOut,

    /** A request answered `401 unauthorized`: expired, revoked or disabled. */
    Expired,
}

sealed interface AuthState {
    /** Reading the stored session. */
    data object Loading : AuthState

    data class SignedIn(
        val session: Session,
    ) : AuthState

    /** [baseUrl] is the last used address (blank on first run); [user] the last user, if any. */
    data class SignedOut(
        val reason: SignedOutReason,
        val baseUrl: String,
        val user: RememberedUser?,
    ) : AuthState
}

/** Where the data of the current user lives. Emits only once the stored session has been read. */
interface UserScopes {
    /**
     * The scope that owns local data: the signed-in user, or, after the session ended, the last one (their queued
     * data stays theirs). Null when nobody ever signed in.
     */
    val scope: Flow<UserScope?>

    suspend fun current(): UserScope?
}

/** Single place that knows who is signed in. Implemented in `:core:data`. */
interface SessionRepository : UserScopes {
    val state: StateFlow<AuthState>

    /** Waits for the stored session to be read. */
    suspend fun awaitReady(): AuthState

    /** The session is valid: saved, with the address and the user. Local data of the legacy install goes to the user. */
    suspend fun signIn(
        baseUrl: String,
        token: String,
        user: AuthUser,
    )

    /** The user chose to leave: the token goes, address, user and local data stay. */
    suspend fun signOut()

    /**
     * A request sent with [token] got `401`. Drops the session only if [token] is still the current one (a late answer to
     * an old request must not end a newer session). True when it ended the session.
     */
    suspend fun expire(token: String): Boolean

    /** Fresh user data from `GET /me` or a key change; ignored if the signed-in user is not [user]. */
    suspend fun updateUser(user: AuthUser)
}

/**
 * Coroutine context element: "this work belongs to [scope]". The HTTP client refuses to send a request whose session
 * is not the one of [scope], so a flush started for user A can never go out with the token of user B.
 */
class ExpectedUserScope(
    val scope: UserScope,
) : AbstractCoroutineContextElement(Key) {
    companion object Key : CoroutineContext.Key<ExpectedUserScope>
}

/** Local data that existed before accounts (single-user install): handed to the first user who signs in. */
fun interface LegacyDataClaimer {
    suspend fun claim(scope: UserScope)
}

/** A [UserScopes] whose answer can be changed by hand: tests and previews. */
class ManualUserScopes(
    initial: UserScope? = UserScope("user-a", "http://teach.test"),
) : UserScopes {
    private val state = kotlinx.coroutines.flow.MutableStateFlow(initial)

    override val scope: Flow<UserScope?> = state

    override suspend fun current(): UserScope? = state.value

    fun switchTo(scope: UserScope?) {
        state.value = scope
    }
}

/**
 * Emits once per move into [AuthState.SignedIn]: the start with a stored session, a login, and the same user coming back after
 * a `401` (`SignedIn(A)`, `SignedOut`, `SignedIn(A)` is two emissions, not one). Refreshing the user data of a live session
 * emits nothing.
 */
fun Flow<AuthState>.signInTransitions(): Flow<Session> =
    kotlinx.coroutines.flow.flow {
        var wasSignedIn = false
        collect { state ->
            val now = state is AuthState.SignedIn
            if (now && !wasSignedIn) emit((state as AuthState.SignedIn).session)
            if (state !is AuthState.Loading) wasSignedIn = now
        }
    }
