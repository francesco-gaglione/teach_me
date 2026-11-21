package com.teachme.feature.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.time.Instant
import java.time.ZoneOffset
import java.util.Locale

class LicensesAndFormatTest {
    private val sample =
        """
        {
          "libraries": [
            {"uniqueId": "b:b", "name": "Beta", "artifactVersion": "2.0", "website": "https://b.example",
             "developers": [{"name": "Bea"}], "licenses": ["Apache-2.0"], "funding": []},
            {"uniqueId": "a:a", "artifactVersion": "1.0", "licenses": ["Weird"]},
            {"uniqueId": "c:c", "name": "Gamma", "artifactVersion": "3", "licenses": []}
          ],
          "licenses": {
            "Apache-2.0": {"name": "Apache License 2.0", "url": "https://apache.org", "content": "TEXT"},
            "Weird": {"name": "Weird License", "url": "https://weird.example"}
          }
        }
        """.trimIndent()

    @Test
    fun `generated format parses, sorts and resolves texts`() {
        val entries = parseLicenses(sample)!!
        assertEquals(listOf("a:a", "Beta", "Gamma"), entries.map { it.name })
        val beta = entries[1]
        assertEquals("Apache License 2.0", beta.licenseName)
        assertEquals("TEXT", beta.text)
        assertEquals("Bea", beta.copyright)
    }

    @Test
    fun `license without text still shows its name and link`() {
        val weird = parseLicenses(sample)!!.first()
        assertEquals("Weird License", weird.licenseName)
        assertTrue(weird.text.isBlank())
        assertEquals("https://weird.example", weird.url)
    }

    @Test
    fun `assembled asset is generated, not hand written`() {
        // The generated file is exported by :app (release runtime classpath); the asset lives in the app, not here.
        assertTrue(!File("src/main/assets/$LICENSES_ASSET").exists())
    }

    @Test
    fun `missing or malformed file means not available`() {
        assertNull(parseLicenses(null))
        assertNull(parseLicenses("not json"))
        assertNull(parseLicenses("""{"licenses":{},"libraries":[]}"""))
    }

    @Test
    fun `range collapses to one date on the same day`() {
        val a = Instant.parse("2026-11-09T10:00:00Z")
        val b = Instant.parse("2026-11-12T10:00:00Z")
        assertEquals("9 nov - 12 nov", formatRange(a, b, ZoneOffset.UTC, Locale.ITALIAN))
        assertEquals("9 nov", formatRange(a, a.plusSeconds(60), ZoneOffset.UTC, Locale.ITALIAN))
        assertEquals("Nov 9 - Nov 12", formatRange(a, b, ZoneOffset.UTC, Locale.US))
        assertNull(formatRange(null, null))
    }
}
