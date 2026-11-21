package com.teachme.sdui.model

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ExerciseCodeParsingTest {
    private fun block(json: String): Block =
        decodeBlock(Json { ignoreUnknownKeys = true }, Json.parseToJsonElement(json), false)

    private fun codeOf(b: Block): ExerciseCode? =
        when (b) {
            is QuizSingle -> b.code
            is QuizMulti -> b.code
            is OpenAnswer -> b.code
            is Flashcard -> b.code
            else -> error("not an exercise with code: $b")
        }

    private fun exercises(codeField: String) =
        listOf(
            """{"type":"quiz_single","id":"a","question":"q","options":[{"id":"x","text":"t"}],"correct":"x"$codeField}""",
            """{"type":"quiz_multi","id":"a","question":"q","options":[{"id":"x","text":"t"}],"correct":["x"]$codeField}""",
            """{"type":"open_answer","id":"a","question":"q"$codeField}""",
            """{"type":"flashcard","id":"a","front":"f","back":"b"$codeField}""",
        )

    @Test
    fun `code is read by the four exercises`() {
        for (json in exercises(""","code":{"language":"rust","code":"let r = &s;\nlet t = 1;"}""")) {
            assertEquals(ExerciseCode("rust", "let r = &s;\nlet t = 1;"), codeOf(block(json)))
        }
    }

    @Test
    fun `a lesson without code parses as before`() {
        for (json in exercises("")) assertNull(codeOf(block(json)))
    }

    @Test
    fun `a malformed code is dropped and the exercise survives`() {
        val malformed =
            listOf(
                ""","code":"let x = 1;"""",
                ""","code":42""",
                ""","code":null""",
                ""","code":{"language":"rust"}""",
                ""","code":{"language":"rust","code":7}""",
                ""","code":{"language":"rust","code":"  "}""",
                ""","code":[1,2]""",
            )
        for (field in malformed) {
            for (json in exercises(field)) {
                val b = block(json)
                assertTrue("$field -> $b", b !is Unavailable)
                assertNull("$field", codeOf(b))
            }
        }
    }

    @Test
    fun `a missing language falls back to text and extra keys are ignored`() {
        val b = block(exercises(""","code":{"code":"x","caption":"c","highlight_lines":[1]}""").first())
        assertEquals(ExerciseCode("text", "x"), codeOf(b))
    }
}
