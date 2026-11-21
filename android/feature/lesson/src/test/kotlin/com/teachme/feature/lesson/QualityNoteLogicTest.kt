package com.teachme.feature.lesson

import com.teachme.core.model.QualityWarning
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class QualityNoteLogicTest {
    private fun w(
        code: String,
        block: String? = null,
        text: String = code,
    ) = QualityWarning(code, block, text)

    @Test
    fun `empty list gives an empty note`() {
        val note = buildQualityNote(emptyList())
        assertEquals(0, note.total)
        assertEquals(emptyList<QualityNoteEntry>(), note.entries)
        assertEquals(0, note.hidden)
    }

    @Test
    fun `a code repeated twice stays as two lines and three folds into one`() {
        val two = buildQualityNote(listOf(w("a", "b1"), w("a", "b2"), w("b", "b3")))
        assertEquals(3, two.entries.size)
        val three = buildQualityNote(listOf(w("a", "b1"), w("c", "b9"), w("a", "b2"), w("a", "b3")))
        assertEquals(2, three.entries.size)
        assertEquals(QualityNoteEntry("a", "a", "b1", 3), three.entries[0])
        assertEquals("c", three.entries[1].code)
        assertEquals(4, three.total)
    }

    @Test
    fun `more than six lines are cut and the rest is counted`() {
        val note = buildQualityNote((1..9).map { w("code$it", "b$it") })
        assertEquals(6, note.entries.size)
        assertEquals(3, note.hidden)
        assertEquals("code1", note.entries.first().code)
    }

    @Test
    fun `block ids that are not in the document are dropped and a group takes the first reachable one`() {
        val note =
            buildQualityNote(
                listOf(w("a", "gone"), w("a", "here"), w("a", null), w("z", "gone")),
            ) { it == "here" }
        assertEquals("here", note.entries[0].blockId)
        assertNull(note.entries[1].blockId)
    }
}
