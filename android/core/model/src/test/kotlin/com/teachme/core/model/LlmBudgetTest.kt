package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.time.ZoneId

class LlmBudgetTest {
    private val rome = ZoneId.of("Europe/Rome")
    private val newYork = ZoneId.of("America/New_York")

    @Test
    fun `only the budget code is a budget code`() {
        assertTrue("llm_budget_exceeded".isLlmBudgetCode())
        assertFalse("rate_limited".isLlmBudgetCode())
        assertFalse(null.isLlmBudgetCode())
    }

    @Test
    fun `without retry-after the reset is the next midnight UTC in the local zone`() {
        // 2026-11-22 18:00 UTC -> 2026-11-23 00:00 UTC -> 01:00 in Rome (CET), the next local day.
        val reset = budgetReset(Instant.parse("2026-11-22T18:00:00Z"), rome)
        assertEquals(1, reset.at.hour)
        assertEquals(0, reset.at.minute)
        assertTrue(reset.tomorrow)
    }

    @Test
    fun `west of UTC the reset can still be today`() {
        // 18:00 UTC is 13:00 in New York; midnight UTC is 19:00 there, the same local day.
        val reset = budgetReset(Instant.parse("2026-11-22T18:00:00Z"), newYork)
        assertEquals(19, reset.at.hour)
        assertFalse(reset.tomorrow)
    }

    @Test
    fun `retry-after wins when present`() {
        val now = Instant.parse("2026-11-22T22:00:00Z")
        val reset = budgetReset(now, rome, retryAfterSecs = 3600)
        assertEquals(Instant.parse("2026-11-22T23:00:00Z"), reset.at.toInstant())
        // A zero or missing value falls back to the next midnight UTC.
        assertEquals(Instant.parse("2026-11-23T00:00:00Z"), budgetReset(now, rome, 0).at.toInstant())
        assertEquals(Instant.parse("2026-11-23T00:00:00Z"), budgetReset(now, rome, null).at.toInstant())
    }

    @Test
    fun `a failure carries its reset instant, from Retry-After or the next midnight UTC`() {
        val now = Instant.parse("2026-11-22T18:00:00Z")
        val nowMs = now.toEpochMilli()
        assertEquals(now.plusSeconds(3600), ApiFailure.LlmBudgetExceeded(3600).budgetResetAt(nowMs))
        assertEquals(Instant.parse("2026-11-23T00:00:00Z"), ApiFailure.LlmBudgetExceeded(null).budgetResetAt(nowMs))
        assertEquals(Instant.parse("2026-11-23T00:00:00Z"), ApiFailure.LlmBudgetExceeded(0).budgetResetAt(nowMs))
        assertNull(ApiFailure.RateLimited(60).budgetResetAt(nowMs))
        assertNull(ApiFailure.Conflict.budgetResetAt(nowMs))
    }

    @Test
    fun `the wait is over exactly at the reset instant, and never for a null one`() {
        val at = Instant.parse("2026-11-23T00:00:00Z")
        assertFalse(budgetRenewed(at, at.minusMillis(1)))
        assertTrue(budgetRenewed(at, at))
        assertTrue(budgetRenewed(at, at.plusSeconds(5)))
        assertTrue(budgetRenewed(null, at))
    }

    @Test
    fun `a failed job counts its wait from its own failure time, a later refusal wins`() {
        val failed = Instant.parse("2026-11-22T18:00:00Z")
        val job =
            TopicJob(
                "j1",
                "t1",
                "lesson_generation",
                JobStatus.Failed,
                null,
                JobError("llm_budget_exceeded", ""),
                null,
                1,
                failed.minusSeconds(60),
                failed.minusSeconds(30),
                failed,
            )
        val midnight = Instant.parse("2026-11-23T00:00:00Z")
        assertEquals(midnight, job.jobBudgetReset())
        assertEquals(midnight, job.jobBudgetReset(refused = midnight.minusSeconds(10)))
        assertEquals(midnight.plusSeconds(60), job.jobBudgetReset(refused = midnight.plusSeconds(60)))
        // A job that failed yesterday is already past its reset: the retry is enabled at once.
        val old = job.copy(finishedAt = failed.minusSeconds(2 * 86_400))
        assertTrue(budgetRenewed(old.jobBudgetReset(), failed))
    }
}
