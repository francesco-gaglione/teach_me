package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LlmKeyTest {
    @Test
    fun `the two key failures map to a variant, nothing else does`() {
        assertEquals(LlmKeyVariant.Missing, ApiFailure.LlmKeyMissing.llmKeyVariant())
        assertEquals(LlmKeyVariant.Rejected, ApiFailure.LlmKeyRejected.llmKeyVariant())
        assertEquals(LlmKeyVariant.Credit, ApiFailure.LlmCreditExhausted.llmKeyVariant())
        assertNull(ApiFailure.Conflict.llmKeyVariant())
        assertNull(ApiFailure.Unexpected(502).llmKeyVariant())
        assertNull(ApiFailure.Unauthorized.llmKeyVariant())
    }

    @Test
    fun `the error codes of a failed job or message map to a variant`() {
        assertEquals(LlmKeyVariant.Missing, "llm_key_missing".llmKeyVariantOfCode())
        assertEquals(LlmKeyVariant.Rejected, "llm_key_rejected".llmKeyVariantOfCode())
        assertEquals(LlmKeyVariant.Credit, "llm_credit_exhausted".llmKeyVariantOfCode())
        assertNull("upstream_failed".llmKeyVariantOfCode())
        assertNull(null.llmKeyVariantOfCode())
    }

    @Test
    fun `suggestions follow the known state of the key`() {
        assertNull(LlmKeyStatus(LlmKeyState.Set).hintVariant)
        assertFalse(LlmKeyStatus(LlmKeyState.Set).needsKey)
        assertEquals(LlmKeyVariant.Missing, LlmKeyStatus(LlmKeyState.Missing).hintVariant)
        assertEquals(LlmKeyVariant.Rejected, LlmKeyStatus(LlmKeyState.RejectedKnown).hintVariant)
        assertTrue(LlmKeyStatus(LlmKeyState.RejectedKnown).needsKey)
    }

    @Test
    fun `credit gives no hint, and an open answer waits with the key problems first`() {
        val seen = LlmKeyStatus(LlmKeyState.Set, creditExhaustedSeen = true)
        assertNull(seen.hintVariant)
        assertFalse(seen.needsKey)
        assertEquals(LlmKeyVariant.Credit, seen.gradeBlocker)
        assertNull(LlmKeyStatus(LlmKeyState.Set).gradeBlocker)
        // Missing > rejected > credit (llm-key.md 9.5).
        assertEquals(LlmKeyVariant.Missing, LlmKeyStatus(LlmKeyState.Missing, creditExhaustedSeen = true).gradeBlocker)
        assertEquals(
            LlmKeyVariant.Rejected,
            LlmKeyStatus(LlmKeyState.RejectedKnown, creditExhaustedSeen = true).gradeBlocker,
        )
    }

    @Test
    fun `a second credit notice from the same source is a repeat, any other notice resets it`() {
        val source = KeyNoticeSource()
        val first = source.of(ApiFailure.LlmCreditExhausted)!!
        val second = source.of(ApiFailure.LlmCreditExhausted)!!
        assertFalse(first.repeated)
        assertTrue(second.repeated)
        assertFalse(source.of(ApiFailure.LlmKeyRejected)!!.repeated)
        assertFalse(source.of(ApiFailure.LlmCreditExhausted)!!.repeated)
    }

    private class SeenMonitor : LlmKeyMonitor {
        val flow = kotlinx.coroutines.flow.MutableStateFlow(LlmKeyStatus())
        override val status get() = flow

        override fun noteObserved(
            variant: LlmKeyVariant,
            at: java.time.Instant,
        ) {
            if (variant == LlmKeyVariant.Credit) flow.value = flow.value.copy(creditExhaustedSeen = true)
        }

        override fun noteSaved(serverDate: java.time.Instant?) = Unit

        override fun noteRemoved() = Unit

        override fun noteWorked(at: java.time.Instant) {
            flow.value = flow.value.copy(creditExhaustedSeen = false)
        }
    }

    @Test
    fun `a credit notice reports to the monitor, and one after a success is new, not a repeat`() {
        val monitor = SeenMonitor()
        val source = KeyNoticeSource(monitor)
        assertFalse(source.of(ApiFailure.LlmCreditExhausted)!!.repeated)
        assertTrue(monitor.status.value.creditExhaustedSeen)
        assertTrue(source.of(ApiFailure.LlmCreditExhausted)!!.repeated)

        monitor.noteWorked()
        assertFalse(source.of(ApiFailure.LlmCreditExhausted)!!.repeated)
    }

    @Test
    fun `every key notice reports its own variant to the monitor, once, when it is born`() {
        val seen = mutableListOf<LlmKeyVariant>()
        val monitor =
            object : LlmKeyMonitor by NoLlmKeyMonitor {
                // KeyNoticeSource reports through noteFailedNow; a delegated default would bypass an override of
                // noteObserved alone.
                override fun noteFailedNow(
                    variant: LlmKeyVariant,
                    serverDate: java.time.Instant?,
                ) {
                    seen += variant
                }
            }
        val source = KeyNoticeSource(monitor)
        source.of(ApiFailure.LlmKeyRejected)
        source.of(ApiFailure.LlmKeyMissing)
        source.of(ApiFailure.LlmCreditExhausted)
        source.of(ApiFailure.Conflict)
        assertEquals(listOf(LlmKeyVariant.Rejected, LlmKeyVariant.Missing, LlmKeyVariant.Credit), seen)
    }

    @Test
    fun `a refusal does not report the credit`() {
        val monitor = SeenMonitor()
        KeyNoticeSource(monitor).of(ApiFailure.LlmKeyRejected)
        assertFalse(monitor.status.value.creditExhaustedSeen)
    }

    private fun job(
        kind: String,
        status: JobStatus,
    ) = TopicJob("j", "t", kind, status, null, null, null, 1, java.time.Instant.EPOCH, null, null)

    @Test
    fun `only a finished LLM operation proves the key works`() {
        assertTrue(ChatEvent.JobUpdated(job("research", JobStatus.Succeeded)).provesKeyWorks())
        assertTrue(ChatEvent.JobUpdated(job("lesson_completion", JobStatus.Succeeded)).provesKeyWorks())
        assertFalse(ChatEvent.JobUpdated(job("research", JobStatus.Running)).provesKeyWorks())
        assertFalse(ChatEvent.JobUpdated(job("research", JobStatus.Failed)).provesKeyWorks())
        // Reading a PDF does not use the LLM.
        assertFalse(ChatEvent.JobUpdated(job("document_import", JobStatus.Succeeded)).provesKeyWorks())
        assertFalse(ChatEvent.ResyncRequired.provesKeyWorks())
    }

    @Test
    fun `every refusal is a new notice`() {
        val source = KeyNoticeSource()
        val first = source.of(LlmKeyVariant.Missing)
        val second = source.of(ApiFailure.LlmKeyMissing)!!

        assertTrue(second.id > first.id)
        assertNull(source.of(ApiFailure.Conflict))
    }
}
