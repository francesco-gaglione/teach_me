package com.teachme.sdui.ink

import androidx.compose.ui.input.pointer.PointerType
import org.junit.Assert.assertEquals
import org.junit.Test

class PenEraseTest {
    @Test
    fun `the eraser end and the side button always erase`() {
        for (switch in listOf(false, true)) {
            assertEquals(true, penErases(PointerType.Eraser, sideButton = false, eraserSwitch = switch))
            assertEquals(true, penErases(PointerType.Stylus, sideButton = true, eraserSwitch = switch))
        }
    }

    @Test
    fun `the stylus tip erases only with the switch on`() {
        assertEquals(false, penErases(PointerType.Stylus, sideButton = false, eraserSwitch = false))
        assertEquals(true, penErases(PointerType.Stylus, sideButton = false, eraserSwitch = true))
    }

    @Test
    fun `a finger and a mouse never erase with the switch`() {
        for (type in listOf(PointerType.Touch, PointerType.Mouse)) {
            assertEquals(false, penErases(type, sideButton = false, eraserSwitch = true))
        }
    }
}
