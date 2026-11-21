package com.teachme.feature.chat

import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.ChatEvent
import com.teachme.core.model.ChatMessageStatus
import com.teachme.core.model.ChatRole
import com.teachme.core.model.ChatStreamSignal
import com.teachme.core.model.LlmKeyMonitor
import com.teachme.core.model.LlmKeyStatus
import com.teachme.core.model.LlmKeyVariant
import com.teachme.core.model.LoadError
import com.teachme.core.model.MessagePart
import com.teachme.core.model.NoLlmKeyMonitor
import com.teachme.core.model.SentMessages
import com.teachme.core.model.Topic
import com.teachme.core.model.TopicState
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class ChatViewModelTest {
    private val repo = FakeChatRepository()
    private val user1 = msg("u1", ChatRole.User, text = "Ciao", second = 1)
    private val a2 = msg("a2", status = ChatMessageStatus.Streaming, text = null, second = 2)

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private class RecordingMonitor : LlmKeyMonitor by NoLlmKeyMonitor {
        var worked = 0
        val observed = mutableListOf<LlmKeyVariant>()
        private val flow = kotlinx.coroutines.flow.MutableStateFlow(LlmKeyStatus())
        override val status: kotlinx.coroutines.flow.StateFlow<LlmKeyStatus> get() = flow

        // Delegation (`by`) does not use the interface default: forward it.
        override fun noteFailedNow(
            variant: LlmKeyVariant,
            serverDate: java.time.Instant?,
        ) = noteObserved(variant, serverDate ?: java.time.Instant.now())

        override fun noteObserved(
            variant: LlmKeyVariant,
            at: java.time.Instant,
        ) {
            observed += variant
            if (variant == LlmKeyVariant.Credit) flow.value = flow.value.copy(creditExhaustedSeen = true)
        }

        override fun noteWorked(at: java.time.Instant) {
            worked++
            flow.value = flow.value.copy(creditExhaustedSeen = false)
        }
    }

    private val keys = RecordingMonitor()

    private fun vm() = ChatViewModel("t1", repo, keys)

    private fun event(
        id: Long,
        e: ChatEvent,
    ) = ChatStreamSignal.Event(id, e)

    private fun text(m: com.teachme.core.model.ChatMessage?) = (m?.parts?.singleOrNull() as? MessagePart.Text)?.text

    /** Starts the screen with [history] (newest first) and an open stream. */
    private suspend fun TestScope.started(
        history: List<com.teachme.core.model.ChatMessage> = emptyList(),
    ): ChatViewModel {
        repo.list = { ApiResult.Success(page(history, null)) }
        val vm = vm()
        vm.onStart()
        advanceUntilIdle()
        repo.push(ChatStreamSignal.Opened)
        advanceUntilIdle()
        return vm
    }

    @Test
    fun `loads the history first oldest first then opens the stream`() =
        runTest {
            val vm = started(listOf(a2.copy(status = ChatMessageStatus.Complete), user1))
            assertEquals(listOf(null), repo.openedWith)
            assertEquals(ChatLoad.Ready, vm.state.value.load)
            assertEquals(
                listOf("u1", "a2"),
                vm.state.value.messages
                    .map { it.id },
            )
            assertFalse(vm.state.value.turnInProgress)
        }

    @Test
    fun `first load error shows the failure and retry recovers`() =
        runTest {
            repo.list = { ApiResult.Failure(ApiFailure.Unauthorized) }
            val vm = vm()
            vm.onStart()
            advanceTimeBy(100)
            assertEquals(ChatLoad.Failed(LoadError.Unauthorized), vm.state.value.load)
            assertTrue(repo.connections.isEmpty())
            repo.list = { ApiResult.Success(page(listOf(user1))) }
            vm.retryLoad()
            advanceTimeBy(100)
            assertEquals(ChatLoad.Ready, vm.state.value.load)
            assertEquals(1, repo.connections.size)
        }

    @Test
    fun `history loads even if the stream never opens`() =
        runTest {
            repo.list = { ApiResult.Success(page(listOf(user1), null)) }
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.ConnectionRefused)))
            advanceTimeBy(100)
            assertEquals(ChatLoad.Ready, vm.state.value.load)
            assertEquals(ChatConnection.Reconnecting, vm.state.value.connection)
        }

    @Test
    fun `stream events build the streaming message and completion replaces it`() =
        runTest {
            val vm = started(listOf(user1))
            repo.push(event(1, ChatEvent.MessageCreated(a2)))
            advanceUntilIdle()
            assertTrue(vm.state.value.turnInProgress)
            repo.push(event(2, ChatEvent.MessageDelta("a2", 0, "Per")))
            repo.push(event(3, ChatEvent.MessageDelta("a2", 0, "ché?")))
            advanceUntilIdle()
            assertEquals(
                "Perché?",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            repo.push(event(4, ChatEvent.MessageCompleted(msg("a2", text = "Perché vuoi?", second = 2, updated = 9))))
            advanceUntilIdle()
            assertEquals(
                "Perché vuoi?",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            assertFalse(vm.state.value.turnInProgress)
            assertEquals(
                AnnouncementKind.Answered,
                vm.state.value.announcement
                    ?.kind,
            )
        }

    @Test
    fun `deltas are applied in batches of at most one per 100 ms`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.push(event(1, ChatEvent.MessageDelta("a2", 0, "a")))
            advanceTimeBy(50)
            repo.push(event(2, ChatEvent.MessageDelta("a2", 0, "b")))
            advanceTimeBy(40)
            assertEquals(
                null,
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            advanceTimeBy(20)
            assertEquals(
                "ab",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            repo.push(event(3, ChatEvent.MessageDelta("a2", 0, "c")))
            advanceTimeBy(99)
            assertEquals(
                "ab",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            advanceTimeBy(2)
            assertEquals(
                "abc",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
        }

    @Test
    fun `a completion flushes pending deltas first so order is kept`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.push(event(1, ChatEvent.MessageDelta("a2", 0, "x")))
            repo.push(event(2, ChatEvent.MessageCompleted(msg("a2", text = "x y", second = 2, updated = 9))))
            advanceTimeBy(1)
            assertEquals(
                "x y",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            advanceTimeBy(500)
            assertEquals(
                "x y",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
        }

    @Test
    fun `topic_updated updates the topic of the state`() =
        runTest {
            val vm = started()
            val topic = Topic("t1", "f", "Rust", TopicState.Interviewing, Instant.EPOCH, Instant.EPOCH)
            repo.push(event(1, ChatEvent.TopicUpdated(topic)))
            advanceUntilIdle()
            assertEquals(topic, vm.state.value.topic)
        }

    @Test
    fun `reconnects with backoff and Last-Event-ID, then clears the reconnecting state`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.push(event(7, ChatEvent.MessageDelta("a2", 0, "ab")))
            advanceUntilIdle()
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(10)
            assertEquals(ChatConnection.Reconnecting, vm.state.value.connection)
            assertEquals(1, repo.connections.size)
            advanceTimeBy(1_000)
            assertEquals(listOf(null, 7L), repo.openedWith)
            // Second failure waits 2 s, then 5 s, then 10 s forever.
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(1_999)
            assertEquals(2, repo.connections.size)
            advanceTimeBy(2)
            assertEquals(3, repo.connections.size)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(5_001)
            assertEquals(4, repo.connections.size)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(10_001)
            assertEquals(5, repo.connections.size)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(10_001)
            assertEquals(6, repo.connections.size)
            repo.push(ChatStreamSignal.Opened)
            advanceUntilIdle()
            assertEquals(ChatConnection.Online, vm.state.value.connection)
            // Missed deltas arrive through the replay and extend the text.
            repo.push(event(8, ChatEvent.MessageDelta("a2", 0, "cd")))
            advanceUntilIdle()
            assertEquals(
                "abcd",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            assertEquals(listOf(null, 7L, 7L, 7L, 7L, 7L), repo.openedWith)
        }

    @Test
    fun `after 30 seconds without a connection the unreachable state shows and reconnectNow skips the wait`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(29_000)
            assertEquals(ChatConnection.Reconnecting, vm.state.value.connection)
            advanceTimeBy(1_100)
            assertEquals(ChatConnection.Unreachable, vm.state.value.connection)
            val before = repo.connections.size
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(10)
            vm.reconnectNow()
            advanceTimeBy(10)
            assertEquals(before + 1, repo.connections.size)
        }

    @Test
    fun `resync_required repeats REST then reopens the stream from the new position`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.list = {
                ApiResult.Success(
                    page(listOf(msg("a2", text = "finale", second = 2, updated = 9), user1), position = 500),
                )
            }
            repo.push(event(1, ChatEvent.ResyncRequired))
            advanceTimeBy(10)
            assertEquals(2, repo.listCalls)
            assertEquals(2, repo.connections.size)
            assertEquals(500L, repo.openedWith.last())
            assertEquals(
                listOf("u1", "a2"),
                vm.state.value.messages
                    .map { it.id },
            )
            assertEquals(
                "finale",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
            assertFalse(vm.state.value.turnInProgress)
        }

    @Test
    fun `unauthorized stream stops reconnecting, shows the error and waits for the user`() =
        runTest {
            repo.list = { ApiResult.Failure(ApiFailure.Unauthorized) }
            val vm = vm()
            vm.onStart()
            advanceTimeBy(10)
            // REST fails first: the load error is shown and the loop backs off; make the REST work, stream says 401.
            repo.list = { ApiResult.Success(page(listOf(user1))) }
            vm.retryLoad()
            advanceTimeBy(10)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Unauthorized))
            advanceTimeBy(120_000)
            assertEquals(1, repo.connections.size)
            assertEquals(ChatConnection.Unreachable, vm.state.value.connection)
            vm.reconnectNow()
            advanceTimeBy(10)
            assertEquals(2, repo.connections.size)
        }

    @Test
    fun `after a 401 the screen reloads by itself when the same user signs in again`() =
        runTest {
            repo.list = { ApiResult.Success(page(listOf(user1))) }
            val vm = vm()
            vm.onStart()
            advanceTimeBy(10)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Unauthorized))
            advanceTimeBy(120_000)
            assertEquals(1, repo.connections.size)

            // The login over this screen closes: the screen is shown again (ON_START), with no "Riprova ora".
            vm.onStart()
            advanceTimeBy(10)

            assertEquals(2, repo.connections.size)
        }

    @Test
    fun `not found stream stops reconnecting and asks the topic screen to check`() =
        runTest {
            val vm = started(listOf(user1))
            val effect = async { vm.effects.first() }
            repo.push(ChatStreamSignal.Ended(ApiFailure.NotFound))
            advanceTimeBy(60_000)
            assertEquals(ChatEffect.StreamNotFound, effect.await())
            assertEquals(1, repo.connections.size)
        }

    @Test
    fun `a server that closes at once gets a growing backoff`() =
        runTest {
            started(listOf(user1))
            repo.push(ChatStreamSignal.Ended(null))
            advanceTimeBy(1_001)
            assertEquals(2, repo.connections.size)
            repo.push(ChatStreamSignal.Opened)
            repo.push(ChatStreamSignal.Ended(null))
            advanceTimeBy(1_500)
            assertEquals(2, repo.connections.size)
            advanceTimeBy(600)
            assertEquals(3, repo.connections.size)
            repo.push(ChatStreamSignal.Opened)
            repo.push(ChatStreamSignal.Ended(null))
            advanceTimeBy(4_900)
            assertEquals(3, repo.connections.size)
            advanceTimeBy(200)
            assertEquals(4, repo.connections.size)
        }

    @Test
    fun `an event resets the backoff`() =
        runTest {
            started(listOf(user1))
            repo.push(ChatStreamSignal.Ended(null))
            advanceTimeBy(1_001)
            repo.push(ChatStreamSignal.Opened)
            repo.push(
                event(
                    1,
                    ChatEvent.TopicUpdated(
                        Topic("t1", "f", "Rust", TopicState.Interviewing, Instant.EPOCH, Instant.EPOCH),
                    ),
                ),
            )
            repo.push(ChatStreamSignal.Ended(null))
            advanceTimeBy(1_001)
            assertEquals(3, repo.connections.size)
        }

    @Test
    fun `a stale reconnectNow does not skip the next backoff`() =
        runTest {
            val vm = started(listOf(user1))
            vm.reconnectNow() // pressed while connected: nobody is waiting
            advanceTimeBy(10)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(500)
            assertEquals(1, repo.connections.size)
            advanceTimeBy(600)
            assertEquals(2, repo.connections.size)
        }

    @Test
    fun `message_created is an upsert and the replayed position survives reconnection`() =
        runTest {
            val failedMsg = msg("a2", status = ChatMessageStatus.Failed, text = "parz", second = 2, updated = 5)
            val vm = started(listOf(failedMsg, user1))
            // The retry re-announces the same id, streaming and empty.
            val again =
                msg("a2", status = ChatMessageStatus.Streaming, text = null, second = 2, updated = 9, lastEventId = 501)
            repo.push(event(501, ChatEvent.MessageCreated(again)))
            advanceTimeBy(10)
            assertEquals(2, vm.state.value.messages.size)
            assertTrue(vm.state.value.turnInProgress)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(1_100)
            assertEquals(501L, repo.openedWith.last())
        }

    @Test
    fun `snapshot with partial text plus a replay with included and new deltas gives the exact text`() =
        runTest {
            val snapshot =
                msg(
                    "a2",
                    status = ChatMessageStatus.Streaming,
                    text = "Perché vuoi",
                    second = 2,
                    updated = 5,
                    lastEventId = 11,
                )
            repo.list = { ApiResult.Success(page(listOf(snapshot, user1), position = 12)) }
            val vm = vm()
            vm.onStart()
            advanceTimeBy(10)
            // Position = min(X-Last-Event-Id, last_event_id of the streaming messages).
            assertEquals(listOf<Long?>(11), repo.openedWith)
            repo.push(ChatStreamSignal.Opened)
            repo.push(
                event(
                    2,
                    ChatEvent.MessageCreated(
                        msg(
                            "a2",
                            status = ChatMessageStatus.Streaming,
                            text = null,
                            second = 2,
                            updated = 1,
                            lastEventId = 2,
                        ),
                    ),
                ),
            )
            repo.push(event(9, ChatEvent.MessageDelta("a2", 0, "Per")))
            repo.push(event(10, ChatEvent.MessageDelta("a2", 0, "ché ")))
            repo.push(event(11, ChatEvent.MessageDelta("a2", 0, "vuoi")))
            repo.push(event(12, ChatEvent.MessageDelta("a2", 0, " imparare")))
            repo.push(event(13, ChatEvent.MessageDelta("a2", 0, " Rust?")))
            advanceTimeBy(200)
            assertEquals(
                "Perché vuoi imparare Rust?",
                text(
                    vm.state.value.messages
                        .last(),
                ),
            )
        }

    @Test
    fun `the stream position is the stream header when no message is streaming`() =
        runTest {
            repo.list = { ApiResult.Success(page(listOf(user1), position = 77)) }
            val vm = vm()
            vm.onStart()
            advanceTimeBy(10)
            assertEquals(listOf<Long?>(77), repo.openedWith)
            assertEquals(ChatLoad.Ready, vm.state.value.load)
        }

    @Test
    fun `leaving the screen closes the stream and coming back reloads and reconnects without a stale id`() =
        runTest {
            val vm = started(listOf(user1))
            repo.push(event(5, ChatEvent.MessageCreated(a2)))
            advanceUntilIdle()
            vm.onStop()
            advanceTimeBy(60_000)
            assertEquals(1, repo.connections.size)
            vm.onStart()
            advanceUntilIdle()
            repo.push(ChatStreamSignal.Opened)
            advanceUntilIdle()
            assertEquals(listOf(null, null), repo.openedWith)
            assertEquals(2, repo.listCalls)
        }

    @Test
    fun `resync_required id becomes the position for the next reconnection and message_created is an upsert`() =
        runTest {
            val failedMsg = msg("a2", status = ChatMessageStatus.Failed, text = "parz", second = 2, updated = 5)
            val vm = started(listOf(failedMsg, user1))
            repo.push(event(1_700_000_000_000_500L, ChatEvent.ResyncRequired))
            advanceUntilIdle()
            // The retry re-announces the same id, streaming and empty.
            repo.push(
                event(
                    1_700_000_000_000_501L,
                    ChatEvent.MessageCreated(
                        msg("a2", status = ChatMessageStatus.Streaming, text = null, second = 2, updated = 9),
                    ),
                ),
            )
            advanceUntilIdle()
            assertEquals(2, vm.state.value.messages.size)
            assertTrue(vm.state.value.turnInProgress)
            repo.push(ChatStreamSignal.Ended(ApiFailure.Network(UnreachableReason.Other)))
            advanceTimeBy(1_100)
            assertEquals(1_700_000_000_000_501L, repo.openedWith.last())
        }

    @Test
    fun `send success adds both messages and is deduplicated with the stream events`() =
        runTest {
            val vm = started()
            val u = msg("u1", ChatRole.User, text = "Voglio Rust", second = 1)
            repo.send = { ApiResult.Success(SentMessages(u, a2)) }
            vm.send("  Voglio Rust  ")
            advanceUntilIdle()
            assertEquals(listOf("Voglio Rust"), repo.sent)
            assertEquals(
                listOf("u1", "a2"),
                vm.state.value.messages
                    .map { it.id },
            )
            assertTrue(vm.state.value.turnInProgress)
            assertFalse(vm.state.value.canSend)
            assertEquals(
                AnnouncementKind.Writing,
                vm.state.value.announcement
                    ?.kind,
            )
            repo.push(event(1, ChatEvent.MessageCreated(u)))
            repo.push(event(2, ChatEvent.MessageCreated(a2)))
            advanceUntilIdle()
            assertEquals(2, vm.state.value.messages.size)
        }

    @Test
    fun `send is ignored while a turn runs, while empty and when too long`() =
        runTest {
            val vm = started(listOf(user1, a2))
            vm.send("altro")
            vm.send("   ")
            advanceUntilIdle()
            assertTrue(repo.sent.isEmpty())
            val idle = started(listOf(user1))
            idle.send("a".repeat(8001))
            advanceUntilIdle()
            assertTrue(repo.sent.isEmpty())
        }

    @Test
    fun `send network failure keeps the state sendable and reports the error`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            vm.send("ciao")
            advanceUntilIdle()
            assertEquals(SendError.Network, vm.state.value.sendError)
            assertTrue(vm.state.value.canSend)
            assertTrue(
                vm.state.value.messages
                    .isEmpty(),
            )
            vm.dismissSendError()
            assertNull(vm.state.value.sendError)
        }

    @Test
    fun `send maps server error, and a 401 shows no panel at all`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.Unauthorized) }
            vm.send("a")
            advanceUntilIdle()
            // The sign-in opens over the screen (login.md): nothing to say here, and the text stays.
            assertNull(vm.state.value.sendError)
            assertNull(vm.state.value.keyNotice)
            assertFalse(vm.state.value.sending)
            repo.send = { ApiResult.Failure(ApiFailure.Unexpected(500)) }
            vm.send("a")
            advanceUntilIdle()
            assertEquals(SendError.Server, vm.state.value.sendError)
        }

    @Test
    fun `a refused key on send becomes a key notice, not a send error`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.LlmKeyMissing) }
            vm.send("a")
            advanceUntilIdle()
            val first = vm.state.value
            assertNull(first.sendError)
            assertEquals(LlmKeyVariant.Missing, first.keyNotice?.notice?.variant)
            assertFalse(first.keyNotice!!.fromRetry)

            repo.send = { ApiResult.Failure(ApiFailure.LlmKeyRejected) }
            vm.send("a")
            advanceUntilIdle()
            val second = vm.state.value.keyNotice!!
            assertEquals(LlmKeyVariant.Rejected, second.notice.variant)
            // A new refusal is a new notice, so a panel born before it does not turn "fixed" by mistake.
            assertTrue(second.notice.id > first.keyNotice!!.notice.id)
        }

    @Test
    fun `a refused or missing key on send is reported to the monitor when the answer arrives`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.LlmKeyRejected) }
            vm.send("a")
            advanceUntilIdle()
            repo.send = { ApiResult.Failure(ApiFailure.LlmKeyMissing) }
            vm.send("a")
            advanceUntilIdle()
            assertEquals(listOf(LlmKeyVariant.Rejected, LlmKeyVariant.Missing), keys.observed)
        }

    @Test
    fun `an exhausted credit on send is a credit notice that stays while the same text goes out again`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.LlmCreditExhausted) }
            vm.send("a")
            advanceUntilIdle()
            val first = vm.state.value
            assertNull(first.sendError)
            assertEquals(LlmKeyVariant.Credit, first.keyNotice?.notice?.variant)
            assertFalse(first.keyNotice!!.notice.repeated)

            // "Riprova" sends again: the panel stays (it shows "Riprovo...") and the second refusal is a repeat.
            repo.send = { ApiResult.Failure(ApiFailure.LlmCreditExhausted) }
            vm.send("a")
            val during = vm.state.value
            assertTrue(during.sending)
            assertEquals(LlmKeyVariant.Credit, during.keyNotice?.notice?.variant)
            advanceUntilIdle()
            val second =
                vm.state.value.keyNotice!!
                    .notice
            assertTrue(second.repeated)
            assertTrue(second.id > first.keyNotice!!.notice.id)

            // If the next answer is another refusal, the notice is replaced by that one.
            repo.send = { ApiResult.Failure(ApiFailure.LlmKeyRejected) }
            vm.send("a")
            advanceUntilIdle()
            assertEquals(
                LlmKeyVariant.Rejected,
                vm.state.value.keyNotice
                    ?.notice
                    ?.variant,
            )
        }

    @Test
    fun `an exhausted credit panel goes when the message finally goes out`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.LlmCreditExhausted) }
            vm.send("a")
            advanceUntilIdle()
            val u = msg("u1", ChatRole.User, text = "a", second = 1)
            repo.send = { ApiResult.Success(SentMessages(u, a2)) }
            vm.send("a")
            advanceUntilIdle()
            assertNull(vm.state.value.keyNotice)
        }

    @Test
    fun `retry of a failed reply that meets an exhausted credit keeps the message panel and counts the repeat`() =
        runTest {
            val vm = started(listOf(failed, user1))
            repo.retry = { ApiResult.Failure(ApiFailure.LlmCreditExhausted) }
            vm.retry("a2")
            advanceUntilIdle()
            // No second panel in the composer: the failed message's own panel says it.
            assertNull(vm.state.value.keyNotice)
            assertEquals(1, vm.state.value.creditRepeats)
            assertFalse(vm.state.value.retrying)
            vm.retry("a2")
            advanceUntilIdle()
            assertEquals(2, vm.state.value.creditRepeats)

            repo.retry = { ApiResult.Failure(ApiFailure.LlmKeyRejected) }
            vm.retry("a2")
            advanceUntilIdle()
            assertEquals(
                LlmKeyVariant.Rejected,
                vm.state.value.keyNotice
                    ?.notice
                    ?.variant,
            )
            // Another kind of failure ends the run.
            assertEquals(0, vm.state.value.creditRepeats)
        }

    @Test
    fun `a replayed old event does not end the run of credit answers, an operation that worked does`() =
        runTest {
            val vm = started(listOf(failed, user1))
            repo.retry = { ApiResult.Failure(ApiFailure.LlmCreditExhausted) }
            vm.retry("a2")
            advanceUntilIdle()
            assertEquals(1, vm.state.value.creditRepeats)

            // An old completed message delivered again: the monitor keeps the credit flag, so the count stays.
            repo.push(event(9, ChatEvent.MessageCompleted(msg("a0", text = "vecchio", second = 0, updated = 0))))
            advanceUntilIdle()
            assertEquals(1, vm.state.value.creditRepeats)

            // The monitor sees something that worked after the failure and drops the flag: the run ends.
            keys.noteWorked(Instant.now())
            advanceUntilIdle()
            assertEquals(0, vm.state.value.creditRepeats)
        }

    @Test
    fun `409 on send reloads the history and shows the running turn`() =
        runTest {
            val vm = started(listOf(user1))
            repo.send = { ApiResult.Failure(ApiFailure.Conflict) }
            repo.list = { ApiResult.Success(page(listOf(a2, user1), null)) }
            vm.send("ciao")
            advanceUntilIdle()
            assertTrue(vm.state.value.turnInProgress)
            assertNull(vm.state.value.notice)
            assertNull(vm.state.value.sendError)
            assertFalse(vm.state.value.sending)
        }

    @Test
    fun `409 on send with no turn running tells the chat was updated`() =
        runTest {
            val vm = started(listOf(user1))
            repo.send = { ApiResult.Failure(ApiFailure.Conflict) }
            repo.list = { ApiResult.Success(page(listOf(msg("a2", text = "fatto"), user1), null)) }
            vm.send("ciao")
            advanceUntilIdle()
            assertEquals(ComposerNotice.ChatUpdated, vm.state.value.notice)
            assertEquals(2, vm.state.value.messages.size)
        }

    @Test
    fun `a message opened from a lesson carries the lesson id once`() =
        runTest {
            val vm = started()
            val u = msg("u1", ChatRole.User, text = "Non ho capito", second = 1)
            repo.send = { ApiResult.Success(SentMessages(u, a2)) }
            vm.send("Non ho capito", lessonId = "lesson-1")
            advanceUntilIdle()
            assertEquals(listOf<String?>("lesson-1"), repo.sentLessonIds)
        }

    @Test
    fun `a 422 on lesson_id sends the message again without the context`() =
        runTest {
            val vm = started()
            val u = msg("u1", ChatRole.User, text = "Non ho capito", second = 1)
            repo.send = {
                if (repo.sentLessonIds.last() != null) {
                    ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("lesson_id", "not_found"))))
                } else {
                    ApiResult.Success(SentMessages(u, a2))
                }
            }
            vm.send("Non ho capito", lessonId = "gone")
            advanceUntilIdle()
            assertEquals(listOf<String?>("gone", null), repo.sentLessonIds)
            assertNull(vm.state.value.sendError)
            assertEquals(2, vm.state.value.messages.size)
        }

    @Test
    fun `a 422 on the text is not retried without the lesson`() =
        runTest {
            val vm = started()
            repo.send = { ApiResult.Failure(ApiFailure.Validation(listOf(ApiErrorDetail("text", "too_long")))) }
            vm.send("x", lessonId = "lesson-1")
            advanceUntilIdle()
            assertEquals(listOf<String?>("lesson-1"), repo.sentLessonIds)
            assertEquals(SendError.TooLong, vm.state.value.sendError)
        }

    private val failed =
        msg("a2", status = ChatMessageStatus.Failed, text = "parz", second = 2, updated = 5)

    @Test
    fun `only the last failed assistant message is retryable`() =
        runTest {
            val vm = started(listOf(failed, user1))
            assertEquals("a2", vm.state.value.retryableMessageId)
            val newer = msg("u3", ChatRole.User, text = "dopo", second = 3)
            repo.push(event(1, ChatEvent.MessageCreated(newer)))
            advanceUntilIdle()
            assertNull(vm.state.value.retryableMessageId)
        }

    @Test
    fun `retry puts the same message back to streaming without the partial text`() =
        runTest {
            val vm = started(listOf(failed, user1))
            repo.retry =
                {
                    ApiResult.Success(
                        msg("a2", status = ChatMessageStatus.Streaming, text = null, second = 2, updated = 9),
                    )
                }
            vm.retry("a2")
            advanceUntilIdle()
            val last =
                vm.state.value.messages
                    .last()
            assertEquals(ChatMessageStatus.Streaming, last.status)
            assertTrue(last.parts.isEmpty())
            assertEquals(2, vm.state.value.messages.size)
            assertTrue(vm.state.value.turnInProgress)
        }

    @Test
    fun `retry 409 reloads and asks for the updated toast`() =
        runTest {
            val vm = started(listOf(failed, user1))
            repo.retry = { ApiResult.Failure(ApiFailure.Conflict) }
            repo.list =
                { ApiResult.Success(page(listOf(msg("a2", text = "ok", second = 2, updated = 9), user1), null)) }
            vm.retry("a2")
            advanceUntilIdle()
            assertNull(vm.state.value.retryableMessageId)
            assertEquals(ChatEffect.ChatUpdatedToast, vm.effects.first())
        }

    @Test
    fun `message_failed ends the turn and announces it`() =
        runTest {
            val vm = started(listOf(user1, a2))
            repo.push(event(1, ChatEvent.MessageFailed(failed.copy(updatedAt = Instant.ofEpochSecond(8)))))
            advanceUntilIdle()
            assertFalse(vm.state.value.turnInProgress)
            assertEquals(
                AnnouncementKind.Failed,
                vm.state.value.announcement
                    ?.kind,
            )
            assertEquals("a2", vm.state.value.retryableMessageId)
        }

    @Test
    fun `older pages are prepended keeping order and the cursor ends`() =
        runTest {
            val old1 = msg("m1", ChatRole.User, text = "vecchio", second = 0)
            repo.list = { cursor ->
                if (cursor ==
                    null
                ) {
                    ApiResult.Success(page(listOf(user1), "c1"))
                } else {
                    ApiResult.Success(page(listOf(old1), null))
                }
            }
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()
            repo.push(ChatStreamSignal.Opened)
            advanceUntilIdle()
            assertEquals("c1", vm.state.value.olderCursor)
            vm.loadOlder()
            advanceUntilIdle()
            assertEquals(
                listOf("m1", "u1"),
                vm.state.value.messages
                    .map { it.id },
            )
            assertNull(vm.state.value.olderCursor)
        }

    @Test
    fun `older page failure is flagged and can be retried`() =
        runTest {
            var fail = true
            repo.list = { cursor ->
                when {
                    cursor == null -> ApiResult.Success(page(listOf(user1), "c1"))
                    fail -> ApiResult.Failure(ApiFailure.Network(UnreachableReason.Other))
                    else -> ApiResult.Success(page(listOf(msg("m1", text = "v", second = 0)), null))
                }
            }
            val vm = vm()
            vm.onStart()
            advanceUntilIdle()
            repo.push(ChatStreamSignal.Opened)
            advanceUntilIdle()
            vm.loadOlder()
            advanceUntilIdle()
            assertTrue(vm.state.value.olderFailed)
            fail = false
            vm.loadOlder()
            advanceUntilIdle()
            assertFalse(vm.state.value.olderFailed)
            assertEquals(2, vm.state.value.messages.size)
        }

    private val mission =
        com.teachme.core.model.Mission(
            "t1",
            2,
            com.teachme.core.model.MissionStatus.Draft,
            "Perché",
            listOf("a"),
            emptyList(),
            emptyList(),
            Instant.EPOCH,
            null,
            null,
        )

    @Test
    fun `a completed message with a proposal is announced as a mission proposal and keeps the card`() =
        runTest {
            val vm = started(listOf(a2))
            val proposal = MessagePart.MissionProposal(mission, null)
            repo.push(
                event(
                    2,
                    ChatEvent.MessageCompleted(
                        msg("a2", second = 2, updated = 9).copy(parts = listOf(MessagePart.Text("Ecco"), proposal)),
                    ),
                ),
            )
            advanceUntilIdle()
            assertEquals(
                AnnouncementKind.MissionProposed,
                vm.state.value.announcement
                    ?.kind,
            )
            assertEquals(
                proposal,
                vm.state.value.messages
                    .last()
                    .missionProposal,
            )
        }

    @Test
    fun `a completed message with a lesson request is announced as a lesson being prepared`() =
        runTest {
            val vm = started(listOf(a2))
            repo.push(
                event(
                    2,
                    ChatEvent.MessageCompleted(
                        msg("a2", second = 2, updated = 9)
                            .copy(parts = listOf(MessagePart.Text("Preparo."), MessagePart.LessonRequest("j1"))),
                    ),
                ),
            )
            advanceUntilIdle()
            assertEquals(
                AnnouncementKind.LessonRequested,
                vm.state.value.announcement
                    ?.kind,
            )
        }

    @Test
    fun `mission and preference events are forwarded and do not touch the messages`() =
        runTest {
            val vm = started(listOf(msg("a1", second = 1)))
            val received = mutableListOf<ChatEvent>()
            val job =
                backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
                    vm.missionEvents.collect {
                        received +=
                            it
                    }
                }
            advanceUntilIdle()
            val before = vm.state.value.messages
            val pref =
                com.teachme.core.model.Preference(
                    "p1",
                    com.teachme.core.model.PreferenceScope.Global,
                    null,
                    "x",
                    Instant.EPOCH,
                )
            repo.push(event(5, ChatEvent.MissionProposed(mission)))
            repo.push(event(6, ChatEvent.MissionConfirmed(mission)))
            repo.push(event(7, ChatEvent.PreferenceSaved(pref)))
            repo.push(
                event(8, ChatEvent.PreferenceDeleted("p1", com.teachme.core.model.PreferenceScope.Global, null)),
            )
            advanceUntilIdle()
            assertEquals(4, received.size)
            assertTrue(received[0] is ChatEvent.MissionProposed)
            assertEquals(before, vm.state.value.messages)
            job.cancel()
        }

    @Test
    fun `the mission reloads once when the stream (re)opens, also after a resync`() =
        runTest {
            val vm = started()
            val received = mutableListOf<ChatEvent>()
            val job =
                backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
                    vm.missionEvents.collect {
                        received +=
                            it
                    }
                }
            advanceUntilIdle()
            val before = received.count { it == ChatEvent.ResyncRequired }
            repo.push(event(9, ChatEvent.ResyncRequired))
            advanceUntilIdle()
            // The resync itself emits nothing: the reopened stream does, exactly once.
            assertEquals(before, received.count { it == ChatEvent.ResyncRequired })
            repo.push(ChatStreamSignal.Opened)
            advanceUntilIdle()
            assertEquals(before + 1, received.count { it == ChatEvent.ResyncRequired })
            job.cancel()
        }
}
