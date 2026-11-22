package com.teachme.core.model

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HealthCompatibilityTest {
    private fun health(apiVersion: Int) = HealthResponse("ok", "0.1.0", apiVersion, 1, "2026-11-22T10:00:00Z")

    private val json = Json { ignoreUnknownKeys = true }

    private fun decode(apiVersion: String?): HealthResponse {
        val field = if (apiVersion == null) "" else """"api_version": $apiVersion,"""
        return json.decodeFromString(
            HealthResponse.serializer(),
            """{"status":"ok","version":"1",$field"catalog_version":1,"server_time":"t"}""",
        )
    }

    @Test
    fun `api_version is read as an integer, or from the integer part of a string`() {
        assertEquals(1, decode("1").apiVersion)
        assertEquals(2, decode("2.0").apiVersion)
        assertEquals(1, decode("\"1.2\"").apiVersion)
        assertEquals(3, decode("\"3\"").apiVersion)
        assertEquals(1, decode("1.9").apiVersion)
    }

    @Test
    fun `a missing or unreadable api_version is compatible`() {
        assertFalse(decode(null).requiresAppUpdate())
        assertFalse(decode("null").requiresAppUpdate())
        assertFalse(decode("\"abc\"").requiresAppUpdate())
        assertFalse(decode("\"\"").requiresAppUpdate())
        assertTrue(decode("\"2.1\"").requiresAppUpdate())
    }

    @Test
    fun `only a higher major requires an update`() {
        assertFalse(health(SUPPORTED_API_VERSION).requiresAppUpdate())
        assertFalse(health(SUPPORTED_API_VERSION - 1).requiresAppUpdate())
        assertTrue(health(SUPPORTED_API_VERSION + 1).requiresAppUpdate())
        assertTrue(health(2).requiresAppUpdate(supported = 1))
        assertFalse(health(2).requiresAppUpdate(supported = 2))
    }
}
