package com.teachme.sdui.model

import com.teachme.sdui.Fixtures
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LessonParserTest {
    private fun usable(name: String): ParseResult.Usable = LessonParser.parse(Fixtures.text(name)) as ParseResult.Usable

    private fun Block.leaves(): List<Block> =
        if (this is MissingComponent) {
            fallback.flatMap {
                it.leaves()
            }
        } else {
            listOf(this)
        }

    private fun LessonDocument.allBlocks() = blocks.flatMap { it.leaves() }

    @Test
    fun `valid lessons parse without a single unavailable block`() {
        for (name in Fixtures.validLessons) {
            val result = usable(name)
            assertEquals(name, emptyList<Block>(), result.document.allBlocks().filterIsInstance<Unavailable>())
            assertTrue(name, result.events.all { it is SduiEvent.WantedComponent })
            assertEquals(1, result.document.catalogVersion)
            assertFalse(result.document.newerCatalog)
        }
    }

    @Test
    fun `the valid lessons cover every catalog type`() {
        val types =
            Fixtures.validLessons
                .flatMap { usable(it).document.blocks }
                .map { it.type }
                .toSet() +
                Fixtures.validLessons.flatMap { usable(it).document.allBlocks() }.map { it.type }
        assertEquals(CATALOG_V1_TYPES, types)
    }

    @Test
    fun `rust lesson fields are read`() {
        val doc = usable("lesson-rust-ownership.json").document
        assertEquals("Ownership e move", doc.title)
        assertTrue(doc.objective.startsWith("Saper"))
        assertTrue(doc.glossary.isNotEmpty())
        assertTrue(doc.sources.isNotEmpty())
        assertNotNull(doc.blocks.first() as? Heading)
        assertEquals(doc.blocks.first().id, doc.titleBlockId)
        assertTrue(
            doc.blocks
                .filterIsInstance<CodeBlock>()
                .first()
                .code
                .isNotBlank(),
        )
    }

    @Test
    fun `photo lesson keeps missing component with its fallback and accepts the rubric`() {
        val doc = usable("lesson-photo-exposure.json")
        val missing =
            doc.document.blocks
                .filterIsInstance<MissingComponent>()
                .single()
        assertEquals("exposure_simulator", missing.wanted)
        assertTrue(missing.fallback.size >= 2)
        assertTrue(doc.events.any { it is SduiEvent.WantedComponent && it.wanted == "exposure_simulator" })
        assertTrue(
            doc.document.blocks
                .filterIsInstance<OpenAnswer>()
                .single()
                .maxChars > 0,
        )
    }

    @Test
    fun `open answer parses with and without rubric`() {
        val json = Json { ignoreUnknownKeys = true }
        val with = """{"id":"o","type":"open_answer","question":"Perche?","max_chars":300,
            "rubric":"secret","required":false}"""
        val without = """{"id":"o","type":"open_answer","question":"Perche?"}"""
        val a = json.decodeFromString(BlockSerializer, with) as OpenAnswer
        val b = json.decodeFromString(BlockSerializer, without) as OpenAnswer
        assertEquals(300, a.maxChars)
        assertFalse(a.required)
        assertEquals(OpenAnswer.DEFAULT_MAX_CHARS, b.maxChars)
        assertTrue(b.required)
    }

    @Test
    fun `unknown type fixture yields an Unknown block and the rest of the lesson`() {
        val result = usable("unknown-type.json")
        val unavailable =
            result.document.blocks
                .filterIsInstance<Unavailable>()
                .single()
        assertEquals("timeline", unavailable.rawType)
        assertEquals(UnavailableReason.UnknownType, unavailable.reason)
        assertTrue(result.events.any { it is SduiEvent.UnknownBlockType && it.type == "timeline" })
        assertTrue(result.document.blocks.count { it !is Unavailable } > 10)
    }

    @Test
    fun `every invalid fixture is parsed without crashing`() {
        val files = Fixtures.invalid()
        assertTrue(files.size >= 18)
        for (file in files) {
            assertTrue(file.name, LessonParser.parse(file.readText()) is ParseResult.Usable)
        }
    }

    private fun invalid(name: String) = LessonParser.parse(Fixtures.text("invalid/$name.json")) as ParseResult.Usable

    @Test
    fun `invalid schema fixture makes the callout unavailable`() {
        val doc = invalid("schema").document
        assertEquals(
            UnavailableReason.Malformed,
            doc.blocks
                .filterIsInstance<Unavailable>()
                .single()
                .reason,
        )
    }

    @Test
    fun `unknown block type fixture`() {
        val doc = invalid("unknown_block_type").document
        assertEquals(
            "memory_diagram",
            doc.blocks
                .filterIsInstance<Unavailable>()
                .single()
                .rawType,
        )
    }

    @Test
    fun `duplicate id keeps the first block`() {
        val result = invalid("duplicate_id")
        assertTrue(result.events.any { it is SduiEvent.DuplicateBlockId && it.id == "s1_code" })
        assertEquals(1, result.document.blocks.count { it.id == "s1_code" })
        assertTrue(result.document.blocks.first { it.id == "s1_code" } is CodeBlock)
    }

    @Test
    fun `quiz whose correct option does not exist is not renderable`() {
        val q2 = invalid("quiz_correct_missing").document.blocks.first { it.id == "q2" }
        assertTrue(q2 is Unavailable && q2.reason == UnavailableReason.Invalid)
    }

    @Test
    fun `fill blank placeholder mismatch is not renderable`() {
        val doc = invalid("fill_blank_mismatch").document
        assertEquals(1, doc.blocks.filterIsInstance<Unavailable>().size)
        assertTrue(
            doc.blocks.none {
                it is FillBlank &&
                    it.id ==
                    doc.blocks
                        .filterIsInstance<Unavailable>()
                        .single()
                        .id
            },
        )
    }

    @Test
    fun `nested missing component is not expanded`() {
        val missing =
            invalid("nested_missing_component")
                .document.blocks
                .filterIsInstance<MissingComponent>()
                .single()
        assertTrue(missing.fallback.any { it is Unavailable && it.reason == UnavailableReason.NoFallback })
    }

    @Test
    fun `semantic violations of the validator still render`() {
        val names =
            listOf(
                "unknown_citation",
                "unknown_glossary_key",
                "unused_entry",
                "primary_source_count",
                "ask_agent_missing",
                "no_required_exercise",
                "quiz_answer_position",
                "quiz_option_length",
                "too_long",
                "first_block_not_title",
                "section_without_content",
                "markdown",
            )
        for (name in names) assertTrue(name, invalid(name).document.blocks.none { it is Unavailable })
    }

    @Test
    fun `newer catalog is flagged and still parsed`() {
        val text =
            Fixtures
                .text(
                    "lesson-git-staging.json",
                ).replaceFirst(Regex("\"catalog_version\":\\s*1"), "\"catalog_version\": 2")
        val result = LessonParser.parse(text) as ParseResult.Usable
        assertTrue(result.document.newerCatalog)
        assertTrue(result.events.any { it is SduiEvent.NewerCatalog })
    }

    @Test
    fun `unusable documents never throw`() {
        for (text in listOf(
            "not json",
            "[]",
            "{}",
            """{"blocks":"x"}""",
            """{"blocks":[]}""",
            """{"blocks":[{"type":"zzz","id":"a"},5,null,{"id":"b"}]}""",
        )) {
            assertTrue(text, LessonParser.parse(text) is ParseResult.Unusable)
        }
    }

    @Test
    fun `garbage blocks next to a good one leave the good one`() {
        val text = """{"blocks":[5,{"id":"h","type":"heading","level":"x","text":"a"},
            {"id":"p","type":"paragraph","text":"ciao"}]}"""
        val doc = (LessonParser.parse(text) as ParseResult.Usable).document
        assertEquals(3, doc.blocks.size)
        assertEquals(2, doc.blocks.count { it is Unavailable })
        assertTrue(doc.blocks.last() is Paragraph)
    }

    @Test
    fun `blank texts are not renderable`() {
        val text = """{"blocks":[{"id":"p","type":"paragraph","text":"   "},
            {"id":"q","type":"paragraph","text":"ok"}]}"""
        assertTrue((LessonParser.parse(text) as ParseResult.Usable).document.blocks[0] is Unavailable)
    }

    @Test
    fun `highlight lines and lang are read`() {
        val text =
            """{"blocks":[{"id":"c","type":"code","language":"rust","code":"a\nb\nc","highlight_lines":[2],"lang":"en"}]}"""
        val code = (LessonParser.parse(text) as ParseResult.Usable).document.blocks.single() as CodeBlock
        assertEquals(listOf(2), code.highlightLines)
        assertEquals("en", code.lang)
    }
}

class LessonParserRobustnessTest {
    private fun doc(text: String) = (LessonParser.parse(text) as ParseResult.Usable).document

    @Test
    fun `open answer max chars is clamped to the contract range`() {
        for ((raw, expected) in listOf(0 to 50, -5 to 50, 10 to 50, 600 to 600, 99999 to 2000)) {
            val text = """{"blocks":[{"id":"o","type":"open_answer","question":"q","max_chars":$raw}]}"""
            assertEquals(expected, (doc(text).blocks.single() as OpenAnswer).maxChars)
        }
    }

    @Test
    fun `details never carry the raw exception message`() {
        val text =
            """{"blocks":[{"id":"c","type":"callout","variant":"SECRET-RUBRIC","text":"x"},""" +
                """{"id":"p","type":"paragraph","text":"y"}]}"""
        val bad = doc(text).blocks.first() as Unavailable
        assertTrue(bad.detail?.contains("SECRET") != true)
    }

    @Test
    fun `documents over the size limit are unusable`() {
        val big = """{"blocks":[{"id":"p","type":"paragraph","text":"${"x".repeat(MAX_DOCUMENT_CHARS)}"}]}"""
        assertTrue(LessonParser.parse(big) is ParseResult.Unusable)
    }

    private fun withPdfSource(): ParseResult.Usable {
        val root = Json.parseToJsonElement(Fixtures.text("lesson-git-staging.json")) as JsonObject
        val sources = root["sources"] as JsonArray
        val first = sources[0] as JsonObject
        val pdf =
            JsonObject(
                first + ("url" to JsonNull) + ("title" to JsonPrimitive("Pro Git (pp. 40-52)")),
            )
        val patched =
            JsonObject(
                root + ("sources" to JsonArray(listOf<JsonElement>(pdf) + sources.drop(1))),
            )
        return LessonParser.parse(patched.toString()) as ParseResult.Usable
    }

    @Test
    fun `a source with url null is a PDF of the learner and stays in the lesson`() {
        val document = withPdfSource().document
        assertEquals(2, document.sources.size)
        val pdf = document.source("progit_2_2")!!
        assertEquals(null, pdf.url)
        assertTrue(pdf.isUserPdf)
        assertEquals("Pro Git (pp. 40-52)", pdf.title)
        assertFalse(document.source("git_add_docs")!!.isUserPdf)
        // The citation and the primary_source that point at it are still rendered, and the numbers do not move.
        assertEquals(1, document.sourceNumber("progit_2_2"))
        assertTrue(document.blocks.any { it is PrimarySource && it.source == "progit_2_2" })
        assertEquals(emptyList<Block>(), document.blocks.filterIsInstance<Unavailable>())
    }

    @Test
    fun `a missing url field is a malformed source and is dropped, an explicit null is a PDF`() {
        val json =
            """{"catalog_version":1,"title":"T","objective":"Saper fare","estimated_minutes":3,"glossary":[],
              |"sources":[{"key":"a","resource_id":"r1","title":"Libro (p. 4)"},
              |{"key":"p","resource_id":"r2","title":"Mio PDF (pp. 1-2)","url":null},
              |{"key":"b","resource_id":null,"title":"Web","url":"https://example.org/x"}],
              |"blocks":[{"id":"h","type":"heading","level":1,"text":"T"},
              |{"id":"p1","type":"paragraph","text":"Testo[^a] e[^p] e[^b]."}]}
            """.trimMargin()
        val document = (LessonParser.parse(json) as ParseResult.Usable).document
        assertEquals(listOf("p", "b"), document.sources.map { it.key })
        assertEquals(listOf(true, false), document.sources.map { it.isUserPdf })
        assertEquals("https://example.org/x", document.source("b")!!.url)
    }
}
