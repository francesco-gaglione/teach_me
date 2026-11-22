package com.teachme.core.model

import org.junit.Assert.assertEquals
import org.junit.Test

class FormFailureTest {
    private fun validation(vararg details: Pair<String, String>) =
        ApiFailure.Validation(details.map { ApiErrorDetail(it.first, it.second) })

    @Test
    fun `field issues map by field name`() {
        assertEquals(FormError(field = FieldIssue.Duplicate), validation("name" to "duplicate").toFormError("name"))
        assertEquals(FormError(field = FieldIssue.Empty), validation("title" to "empty").toFormError("title"))
        assertEquals(FormError(field = FieldIssue.TooLong), validation("name" to "too_long").toFormError("name"))
    }

    @Test
    fun `issue on another field or unknown issue is unexpected`() {
        val unexpected = FormError(action = ActionFailure.Unexpected)
        assertEquals(unexpected, validation("folder_id" to "invalid").toFormError("title"))
        assertEquals(unexpected, validation("name" to "weird").toFormError("name"))
        assertEquals(unexpected, ApiFailure.Validation(emptyList()).toFormError("name"))
    }

    @Test
    fun `transport and http failures map to action failures`() {
        assertEquals(
            FormError(action = ActionFailure.Network),
            ApiFailure.Network(UnreachableReason.Timeout).toFormError("name"),
        )
        assertEquals(FormError(action = ActionFailure.Unauthorized), ApiFailure.Unauthorized.toFormError("name"))
        assertEquals(FormError(action = ActionFailure.Unexpected), ApiFailure.Unexpected(500).toFormError("name"))
    }

    @Test
    fun `load errors`() {
        assertEquals(LoadError.Network, ApiFailure.Network(UnreachableReason.Other).toLoadError())
        assertEquals(LoadError.Unauthorized, ApiFailure.Unauthorized.toLoadError())
        assertEquals(LoadError.Server, ApiFailure.Unexpected(503).toLoadError())
    }

    @Test
    fun `flash inbox hands a message only to screens that want it`() {
        val inbox = InMemoryFlashInbox()
        inbox.post(Flash(FlashKind.TopicDeleted, "Rust"))
        assertEquals(null, inbox.take(FlashKind.FolderGone))
        assertEquals(Flash(FlashKind.TopicDeleted, "Rust"), inbox.take(FlashKind.TopicDeleted, FlashKind.TopicGone))
        assertEquals(null, inbox.take(FlashKind.TopicDeleted))
    }

    @Test
    fun `a flash that was not consumed in time is discarded`() {
        var clock = 0L
        val inbox = InMemoryFlashInbox(maxAgeMs = 1_000, now = { clock })
        inbox.post(Flash(FlashKind.FolderGone))
        clock = 1_001
        assertEquals(null, inbox.take(FlashKind.FolderGone))
        inbox.post(Flash(FlashKind.FolderGone))
        clock = 1_500
        assertEquals(Flash(FlashKind.FolderGone), inbox.take(FlashKind.FolderGone))
    }

    @Test
    fun `clearing the user flashes keeps only the messages of the sign-in screen`() {
        val inbox = InMemoryFlashInbox()
        inbox.post(Flash(FlashKind.FolderDeleted, "Lavoro di A"))
        inbox.clearUserFlashes()
        assertEquals(null, inbox.take(FlashKind.FolderDeleted))

        inbox.post(Flash(FlashKind.SignedOut))
        inbox.clearUserFlashes()
        assertEquals(Flash(FlashKind.SignedOut), inbox.take(FlashKind.SignedOut))

        inbox.post(Flash(FlashKind.TopicGone, "Rust"))
        inbox.clear()
        assertEquals(null, inbox.take(FlashKind.TopicGone))
    }
}
