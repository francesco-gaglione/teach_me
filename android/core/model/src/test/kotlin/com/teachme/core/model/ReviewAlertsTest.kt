package com.teachme.core.model

import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

class ReviewAlertsTest {
    private val userA = AuthUser("a", "ann", "Ann", hasLlmKey = true)
    private val userB = AuthUser("b", "bob", "Bob", hasLlmKey = true)

    private class FakeSessions(
        initial: AuthState,
    ) : SessionRepository {
        override val state = MutableStateFlow(initial)
        override val scope get() = error("not used")

        override suspend fun current(): UserScope? = null

        override suspend fun awaitReady() = state.first { it !is AuthState.Loading }

        override suspend fun signIn(
            baseUrl: String,
            token: String,
            user: AuthUser,
        ) = error("not used")

        override suspend fun signOut() = error("not used")

        override suspend fun expire(token: String) = error("not used")

        override suspend fun updateUser(user: AuthUser) = error("not used")
    }

    private class FakeReviews : ReviewRepository {
        var result: ApiResult<DueReviews> = ApiResult.Success(dueOf(0))
        var onCall: () -> Unit = {}
        var asked = 0
        var limit = -1

        override suspend fun due(
            limit: Int,
            topicId: String?,
        ): ApiResult<DueReviews> {
            asked++
            this.limit = limit
            onCall()
            return result
        }

        override suspend fun rate(review: QueuedReview) = error("not used")

        override suspend fun undo(cardId: String) = error("not used")

        override suspend fun suspend(cardId: String) = error("not used")

        override suspend fun unsuspend(cardId: String) = error("not used")

        override suspend fun suspended(
            limit: Int,
            cursor: String?,
        ) = error("not used")
    }

    private class FakeReference : ReviewAlertReference {
        val values = HashMap<String, Int>()

        override suspend fun get(scope: UserScope) = values[scope.key] ?: 0

        override suspend fun set(
            scope: UserScope,
            count: Int,
        ) {
            values[scope.key] = count
        }
    }

    private class RecordingNotifier : ReviewNotifier {
        val shown = mutableListOf<Triple<Int, Int, Boolean>>()
        var cancels = 0
        var posts = true

        override fun show(
            dueCount: Int,
            estimatedMinutes: Int,
            alert: Boolean,
        ): Boolean {
            shown += Triple(dueCount, estimatedMinutes, alert)
            return posts
        }

        override fun cancel() {
            cancels++
        }
    }

    private val reviews = FakeReviews()
    private val reference = FakeReference()
    private val notifier = RecordingNotifier()

    private fun signedIn(user: AuthUser) = AuthState.SignedIn(Session("http://teach.test", "tok-${user.id}", user))

    private fun check(state: AuthState = signedIn(userA)) =
        ReviewReminderCheck(FakeSessions(state), reviews, reference, notifier)

    private fun scopeOf(user: AuthUser) = Session("http://teach.test", "t", user).scope

    // The pure rule.

    @Test
    fun growthAlertsAndRemembersTheCount() {
        val d = ReviewAlertRule.decide(dueCount = 5, estimatedMinutes = 3, reference = 2)
        assertEquals(ReviewAlertDecision.Show(5, 3, alert = true), d)
        assertEquals(5, ReviewAlertRule.newReference(d))
    }

    @Test
    fun firstCheckWithDueCardsAlerts() {
        assertEquals(true, (ReviewAlertRule.decide(1, 1, 0) as ReviewAlertDecision.Show).alert)
    }

    @Test
    fun dropUpdatesSilentlyAndLowersTheReference() {
        val d = ReviewAlertRule.decide(dueCount = 2, estimatedMinutes = 1, reference = 5)
        assertEquals(ReviewAlertDecision.Show(2, 1, alert = false), d)
        assertEquals(2, ReviewAlertRule.newReference(d))
    }

    @Test
    fun sameCountDoesNotAlert() {
        assertEquals(false, (ReviewAlertRule.decide(4, 2, 4) as ReviewAlertDecision.Show).alert)
    }

    @Test
    fun zeroClearsAndResetsTheReference() {
        val d = ReviewAlertRule.decide(dueCount = 0, estimatedMinutes = 0, reference = 7)
        assertEquals(ReviewAlertDecision.Clear, d)
        assertEquals(0, ReviewAlertRule.newReference(d))
    }

    // The check, with a fake API.

    @Test
    fun successShowsTheNotificationAndAsksForOneCard() =
        runTest {
            reviews.result = ApiResult.Success(dueOf(3, minutes = 4))
            assertEquals(ReviewCheckOutcome.Done, check().run())
            assertEquals(1, reviews.limit)
            assertEquals(listOf(Triple(3, 4, true)), notifier.shown)
            assertEquals(3, reference.get(scopeOf(userA)))
        }

    @Test
    fun aSecondCheckWithFewerCardsIsSilentAndZeroClears() =
        runTest {
            val check = check()
            reviews.result = ApiResult.Success(dueOf(5))
            check.run()
            reviews.result = ApiResult.Success(dueOf(2))
            check.run()
            assertEquals(false, notifier.shown.last().third)
            assertEquals(2, reference.get(scopeOf(userA)))
            reviews.result = ApiResult.Success(dueOf(0))
            check.run()
            assertEquals(1, notifier.cancels)
            assertEquals(0, reference.get(scopeOf(userA)))
        }

    @Test
    fun networkErrorShowsNothingAndKeepsTheReference() =
        runTest {
            reference.set(scopeOf(userA), 4)
            reviews.result = ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout))
            assertEquals(ReviewCheckOutcome.Failed, check().run())
            assertTrue(notifier.shown.isEmpty())
            assertEquals(0, notifier.cancels)
            assertEquals(4, reference.get(scopeOf(userA)))
        }

    @Test
    fun unauthorizedShowsNothingAndClearsTheNotificationAndReference() =
        runTest {
            reference.set(scopeOf(userA), 4)
            reviews.result = ApiResult.Failure(ApiFailure.Unauthorized)
            assertEquals(ReviewCheckOutcome.SessionEnded, check().run())
            assertTrue(notifier.shown.isEmpty())
            assertEquals(1, notifier.cancels)
            assertEquals(0, reference.get(scopeOf(userA)))
        }

    @Test
    fun noSessionAsksNothing() =
        runTest {
            val state = AuthState.SignedOut(SignedOutReason.LoggedOut, "http://teach.test", null)
            assertEquals(ReviewCheckOutcome.NoSession, check(state).run())
            assertEquals(0, reviews.asked)
            assertEquals(1, notifier.cancels)
        }

    @Test
    fun aColdStartWaitsForTheSessionInsteadOfClearingTheNotification() =
        runTest {
            val sessions = FakeSessions(AuthState.Loading)
            val check = ReviewReminderCheck(sessions, reviews, reference, notifier)
            reviews.result = ApiResult.Success(dueOf(3))
            val outcome = backgroundScope.async { check.run() }
            runCurrent()
            // Still reading the stored session: nothing decided, nothing cancelled.
            assertEquals(0, notifier.cancels)
            assertEquals(0, reviews.asked)
            sessions.state.value = signedIn(userA)
            assertEquals(ReviewCheckOutcome.Done, outcome.await())
            assertEquals(0, notifier.cancels)
            assertEquals(listOf(Triple(3, 0, true)), notifier.shown)
        }

    @Test
    fun aBlockedNotificationDoesNotUseUpTheGrowth() =
        runTest {
            val check = check()
            reference.set(scopeOf(userA), 2)
            notifier.posts = false
            reviews.result = ApiResult.Success(dueOf(5))
            check.run()
            assertEquals(2, reference.get(scopeOf(userA)))
            // Unblocked: the same count still alerts, because it is still a growth over what was notified.
            notifier.posts = true
            check.run()
            assertEquals(true, notifier.shown.last().third)
            assertEquals(5, reference.get(scopeOf(userA)))
        }

    @Test
    fun eachUserHasTheirOwnReference() =
        runTest {
            reviews.result = ApiResult.Success(dueOf(3))
            check(signedIn(userA)).run()
            // User B starts from 0: 3 cards alert again even though A already has a reference of 3.
            check(signedIn(userB)).run()
            assertEquals(listOf(true, true), notifier.shown.map { it.third })
            assertEquals(3, reference.get(scopeOf(userA)))
            assertEquals(3, reference.get(scopeOf(userB)))
        }

    @Test
    fun anAnswerForAUserWhoLeftMeanwhileIsDropped() =
        runTest {
            val sessions = FakeSessions(signedIn(userA))
            val check = ReviewReminderCheck(sessions, reviews, reference, notifier)
            reviews.result = ApiResult.Success(dueOf(3))
            reviews.onCall = { sessions.state.value = signedIn(userB) }
            assertEquals(ReviewCheckOutcome.Failed, check.run())
            assertTrue(notifier.shown.isEmpty())
            assertEquals(0, reference.get(scopeOf(userA)))
        }
}

private fun dueOf(
    count: Int,
    minutes: Int = 0,
) = DueReviews(items = emptyList(), dueCount = count, estimatedMinutes = minutes, nextDueAt = null as Instant?)
