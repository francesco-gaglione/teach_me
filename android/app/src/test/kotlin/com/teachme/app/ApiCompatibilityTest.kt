package com.teachme.app

import com.teachme.core.model.BackendSettings
import com.teachme.core.model.ConnectionResult
import com.teachme.core.model.HealthResponse
import com.teachme.core.model.UnreachableReason
import com.teachme.core.network.HealthApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ApiCompatibilityTest {
    private class FakeHealth(
        var result: ConnectionResult,
    ) : HealthApi {
        override suspend fun check(override: BackendSettings?): ConnectionResult = result
    }

    private fun connected(apiVersion: Int) =
        ConnectionResult.Connected(HealthResponse("ok", "9.9.9", apiVersion, 1, "2026-11-22T10:00:00Z"))

    @Test
    fun `same major is normal`() =
        runBlocking {
            val c = ApiCompatibility(FakeHealth(connected(1)))
            c.check()
            assertFalse(c.updateRequired.value)
        }

    @Test
    fun `a lower major is normal`() =
        runBlocking {
            val c = ApiCompatibility(FakeHealth(connected(0)))
            c.check()
            assertFalse(c.updateRequired.value)
        }

    @Test
    fun `a higher major blocks, and a later normal answer unblocks`() =
        runBlocking {
            val health = FakeHealth(connected(2))
            val c = ApiCompatibility(health)
            c.check()
            assertTrue(c.updateRequired.value)
            health.result = connected(1)
            c.check()
            assertFalse(c.updateRequired.value)
        }

    @Test
    fun `errors keep the last verdict and never throw`() =
        runBlocking {
            val health = FakeHealth(connected(2))
            val c = ApiCompatibility(health)
            c.check()
            health.result = ConnectionResult.Unreachable(UnreachableReason.Timeout)
            c.check()
            health.result = ConnectionResult.SessionEnded
            c.check()
            health.result = ConnectionResult.Unexpected(500)
            c.check()
            assertTrue(c.updateRequired.value)
        }

    @Test
    fun `recheck still outdated announces it once per outcome and stays blocked`() =
        runBlocking {
            val c = ApiCompatibility(FakeHealth(connected(2)))
            c.recheckWithFeedback()
            assertTrue(c.updateRequired.value)
            assertEquals(UpdateCheckState(false, UpdateNotice.StillOutdated, 1), c.recheck.value)
            c.recheckWithFeedback()
            assertEquals(2, c.recheck.value.noticeId)
        }

    @Test
    fun `recheck unreachable keeps the screen and says so`() =
        runBlocking {
            val health = FakeHealth(connected(2))
            val c = ApiCompatibility(health)
            c.check()
            health.result = ConnectionResult.Unreachable(UnreachableReason.Timeout)
            c.recheckWithFeedback()
            assertTrue(c.updateRequired.value)
            assertEquals(UpdateNotice.Unreachable, c.recheck.value.notice)
            assertFalse(c.recheck.value.checking)
        }

    @Test
    fun `recheck that finds a compatible server unblocks without a notice`() =
        runBlocking {
            val health = FakeHealth(connected(2))
            val c = ApiCompatibility(health)
            c.check()
            health.result = connected(1)
            c.recheckWithFeedback()
            assertFalse(c.updateRequired.value)
            assertEquals(null, c.recheck.value.notice)
        }

    @Test
    fun `reset on sign-out clears the verdict and the notice`() =
        runBlocking {
            val c = ApiCompatibility(FakeHealth(connected(2)))
            c.recheckWithFeedback()
            c.reset()
            assertFalse(c.updateRequired.value)
            assertEquals(UpdateCheckState(), c.recheck.value)
        }

    @Test
    fun `a late answer after a reset is dropped, for check and for recheck`() =
        runBlocking {
            val gate = kotlinx.coroutines.CompletableDeferred<ConnectionResult>()
            val health =
                object : HealthApi {
                    override suspend fun check(override: BackendSettings?): ConnectionResult = gate.await()
                }
            val c = ApiCompatibility(health)
            val late = launch(start = kotlinx.coroutines.CoroutineStart.UNDISPATCHED) { c.recheckWithFeedback() }
            val lateCheck = launch(start = kotlinx.coroutines.CoroutineStart.UNDISPATCHED) { c.check() }
            assertTrue(c.recheck.value.checking)
            c.reset()
            gate.complete(connected(2))
            late.join()
            lateCheck.join()
            // The old session's verdict never reaches the new state.
            assertFalse(c.updateRequired.value)
            assertEquals(UpdateCheckState(), c.recheck.value)
        }

    @Test
    fun `unreachable at first does not block`() =
        runBlocking {
            val c = ApiCompatibility(FakeHealth(ConnectionResult.Unreachable()))
            c.check()
            assertFalse(c.updateRequired.value)
        }
}
