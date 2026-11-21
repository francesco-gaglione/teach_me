package com.teachme.core.model

import kotlinx.coroutines.flow.StateFlow
import java.time.Instant

/**
 * Which message the user gets for a key problem (llm-key.md): no key saved, a key the service refused, or a key the
 * service accepted but with no credit left (section 9: that one is not a key problem, nothing to change in Settings).
 */
enum class LlmKeyVariant { Missing, Rejected, Credit }

/**
 * A key problem met by a request a screen made (a refused `409`/`502`): the notice stays until the key changes or the
 * action is tried again. [id] names this occurrence, so a new refusal is a new notice. [repeated] is true for a credit
 * notice that follows another credit notice of the same source: the user tried again and it is still exhausted.
 */
data class KeyNotice(
    val variant: LlmKeyVariant,
    val id: Int,
    val repeated: Boolean = false,
) {
    /** For the panel's announcement: the id of a repeated credit notice (a new value each time), 0 otherwise. */
    val again: Int get() = if (repeated) id else 0
}

/**
 * Hands out [KeyNotice]s with increasing ids. A notice is born from the answer of a request, so it is also where the
 * key problem is reported to [monitor], as of the server's `Date` of the answer when it has one, otherwise as of now. A credit notice is [KeyNotice.repeated] only while the monitor still remembers the exhaustion: a success in between
 * ([LlmKeyMonitor.noteWorked]) or any other notice makes the next one a new event. Without a [monitor] only the
 * sequence of notices decides.
 */
class KeyNoticeSource(
    private val monitor: LlmKeyMonitor? = null,
) {
    private var next = 0
    private var lastWasCredit = false

    fun of(
        variant: LlmKeyVariant,
        serverDate: Instant? = null,
    ): KeyNotice {
        val credit = variant == LlmKeyVariant.Credit
        val repeated = credit && lastWasCredit && (monitor?.status?.value?.creditExhaustedSeen ?: true)
        lastWasCredit = credit
        monitor?.noteFailedNow(variant, serverDate)
        return KeyNotice(variant, ++next, repeated)
    }

    /** The notice for a failed call, or null when the failure is not about the key. */
    fun of(failure: ApiFailure): KeyNotice? = failure.llmKeyVariant()?.let { of(it) }

    /** Same, dated by the `Date` the failed response carried (the server's clock, not the device's). */
    fun of(result: ApiResult.Failure): KeyNotice? = result.failure.llmKeyVariant()?.let { of(it, result.serverDate) }
}

/** Known state of the user's key, derived locally; the backend stays the only authority (llm-key.md section 2). */
enum class LlmKeyState { Missing, Set, RejectedKnown }

/**
 * @property version local counter, raised when the key is saved or removed here or when `GET /me` changes
 * `has_llm_key`: a panel born before a raise knows the user may have fixed the problem.
 */
data class LlmKeyStatus(
    val state: LlmKeyState = LlmKeyState.Set,
    val version: Int = 0,
    val provider: LlmProvider = LlmProvider.Unknown,
    /** A credit-exhausted answer was seen in this run and no LLM operation has worked since (llm-key.md 9.4). */
    val creditExhaustedSeen: Boolean = false,
) {
    /** The state says there is nothing to use: suggestions apply (llm-key.md section 4). */
    val needsKey: Boolean get() = state != LlmKeyState.Set

    /** The variant a proactive suggestion would use; null when the key is fine. */
    val hintVariant: LlmKeyVariant?
        get() =
            when (state) {
                LlmKeyState.Missing -> LlmKeyVariant.Missing
                LlmKeyState.RejectedKnown -> LlmKeyVariant.Rejected
                LlmKeyState.Set -> null
            }
}

/**
 * What an open answer that waits for its grade says (llm-key.md 5.1 and 9.5): [hintVariant] first (missing, then
 * refused), then the credit when it was seen, otherwise null.
 */
val LlmKeyStatus.gradeBlocker: LlmKeyVariant?
    get() = hintVariant ?: LlmKeyVariant.Credit.takeIf { creditExhaustedSeen }

/** `409 llm_key_missing` / `502 llm_key_rejected` / `502 llm_credit_exhausted` as a variant; null for anything else. */
fun ApiFailure.llmKeyVariant(): LlmKeyVariant? =
    when (this) {
        ApiFailure.LlmKeyMissing -> LlmKeyVariant.Missing
        ApiFailure.LlmKeyRejected -> LlmKeyVariant.Rejected
        ApiFailure.LlmCreditExhausted -> LlmKeyVariant.Credit
        else -> null
    }

/** The `error_code` of a failed job or message (api-conventions section 7) as a variant; null for other codes. */
fun String?.llmKeyVariantOfCode(): LlmKeyVariant? =
    when (this) {
        "llm_key_missing" -> LlmKeyVariant.Missing
        "llm_key_rejected" -> LlmKeyVariant.Rejected
        "llm_credit_exhausted" -> LlmKeyVariant.Credit
        else -> null
    }

/**
 * What the app knows about the key of the signed-in user. Every entry point that gets a key problem reports it here
 * ([noteObserved]), and every screen that suggests or explains reads [status].
 */
interface LlmKeyMonitor {
    val status: StateFlow<LlmKeyStatus>

    /**
     * A request, job or message failed because of the key, at [at] (the instant of the failure: the server's for a job or
     * a message, now for the answer of a request that just failed): remember it (a refusal shows in Settings and Home).
     * An observation that is not later than the last proof that the key is fine ([noteSaved], [noteWorked]) is stale and
     * ignored, so the same failure can be reported from any place that sees it, any number of times. A
     * [LlmKeyVariant.Credit] proves the key is accepted: it clears an older remembered refusal and is itself kept only in
     * memory ([LlmKeyStatus.creditExhaustedSeen]).
     */
    fun noteObserved(
        variant: LlmKeyVariant,
        at: Instant = Instant.now(),
    )

    /**
     * The user saved a new key (proof it is fine: clears an older refusal) or it was seen set elsewhere; raises the
     * version. [serverDate] is the `Date` of the response that said so (the
     * server's clock, one-second precision; null when it had none): the proof is dated by it, not by the device clock,
     * so a device clock that runs ahead cannot hide a really new failure. A `Date` that is absurdly far ahead is ignored.
     */
    fun noteSaved(serverDate: Instant? = null)

    fun noteRemoved()

    /**
     * An LLM operation finished well, at [at] (the server's instant when the app has it, now otherwise): the service accepts
     * the key, so a remembered refusal or credit exhaustion older than [at] is stale (for example the key was replaced
     * from the server, `has_llm_key` unchanged), and failures not later than [at] are ignored from now on.
     */
    fun noteWorked(at: Instant = Instant.now())

    /**
     * An LLM operation worked and the answer carries no instant of the server: like [noteWorked] as of the device's clock,
     * but never earlier than the latest key failure already seen, so a clock that runs behind cannot re-arm it.
     */
    fun noteWorkedNow() = noteWorked()

    /**
     * An LLM operation worked and its response carried the server's `Date` ([serverDate], one-second precision): the
     * proof is dated by it. A failure inside that same second counts as not later (the safe side: an old failure replayed
     * after the success must not re-arm the state). Without a date it is [noteWorkedNow].
     */
    fun noteWorkedAtServerDate(serverDate: Instant?) = noteWorkedNow()

    /**
     * A request made now failed because of the key, and the answer carried the server's `Date` ([serverDate], one-second
     * precision, null when it had none). The failure is dated by the START of that second: a proof (a save, an operation
     * that worked) is dated by the END of its second, so a failure and a proof inside the same second count as the
     * failure being older, whichever came first. The safe side again: a replayed or racing old failure never re-arms what
     * was just fixed, and a really new failure in that very second is missed until the next one. Without a date it is
     * [noteObserved] as of the device's clock. Unlike a replayed job or message, this failure is new by construction.
     */
    fun noteFailedNow(
        variant: LlmKeyVariant,
        serverDate: Instant?,
    ) = noteObserved(variant, serverDate?.truncatedTo(java.time.temporal.ChronoUnit.SECONDS) ?: Instant.now())
}

/** The instant a failed job gave up: the closest the contract offers is `finished_at`, then when it started or was created. */
val TopicJob.failedAt: Instant get() = finishedAt ?: startedAt ?: createdAt

/**
 * Reports to the monitor what [job] says about the key: a failure for a key code as of its failure instant, a finished
 * LLM job as proof the key works. Safe to call again and again with the same job (see [LlmKeyMonitor.noteObserved]).
 */
fun LlmKeyMonitor.observeJob(job: TopicJob) {
    when {
        job.status == JobStatus.Failed ->
            job.error
                ?.code
                .llmKeyVariantOfCode()
                ?.let { noteObserved(it, job.failedAt) }
        job.status == JobStatus.Succeeded && !job.isDocumentImport -> noteWorked(job.failedAt)
    }
}

/** Same as [observeJob] for a chat message: `updated_at` is the closest instant the contract gives to the outcome. */
fun LlmKeyMonitor.observeMessage(message: ChatMessage) {
    when (message.status) {
        ChatMessageStatus.Failed ->
            message.error?.code.llmKeyVariantOfCode()?.let {
                noteObserved(
                    it,
                    message.updatedAt,
                )
            }
        ChatMessageStatus.Complete -> if (message.role == ChatRole.Assistant) noteWorked(message.updatedAt)
        ChatMessageStatus.Streaming -> Unit
    }
}

/** Same as [observeJob] for an event of the Topic stream. */
fun LlmKeyMonitor.observeEvent(event: ChatEvent) {
    when (event) {
        is ChatEvent.MessageCompleted -> observeMessage(event.message)
        is ChatEvent.MessageFailed -> observeMessage(event.message)
        is ChatEvent.JobUpdated -> observeJob(event.job)
        else -> Unit
    }
}

/** True for events that prove the user's key works: a completed Tutor message, a succeeded LLM job. */
fun ChatEvent.provesKeyWorks(): Boolean =
    when (this) {
        is ChatEvent.MessageCompleted -> true
        is ChatEvent.JobUpdated -> job.status == JobStatus.Succeeded && !job.isDocumentImport
        else -> false
    }

/** A monitor that never has anything to say, for previews and tests. */
object NoLlmKeyMonitor : LlmKeyMonitor {
    override val status: StateFlow<LlmKeyStatus> = kotlinx.coroutines.flow.MutableStateFlow(LlmKeyStatus())

    override fun noteObserved(
        variant: LlmKeyVariant,
        at: Instant,
    ) = Unit

    override fun noteSaved(serverDate: Instant?) = Unit

    override fun noteRemoved() = Unit

    override fun noteWorked(at: Instant) = Unit
}
