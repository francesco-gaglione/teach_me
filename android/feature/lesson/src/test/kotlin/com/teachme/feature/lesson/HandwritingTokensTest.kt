package com.teachme.feature.lesson

import com.teachme.core.designsystem.TeachHandwriting
import com.teachme.sdui.ink.InkLimits
import com.teachme.sdui.ink.InkWidth
import org.junit.Assert.assertEquals
import org.junit.Test

/** The pure logic of `sdui.ink` keeps its own copy of the design values: it must be the one of the tokens. */
class HandwritingTokensTest {
    @Test
    fun `the limits and measures of the ink logic are the design tokens`() {
        assertEquals(TeachHandwriting.limitMaxStrokes, InkLimits.MAX_STROKES)
        assertEquals(TeachHandwriting.limitMaxPointsPerStroke, InkLimits.MAX_POINTS_PER_STROKE)
        assertEquals(TeachHandwriting.limitMaxPointsPerLesson, InkLimits.MAX_POINTS)
        assertEquals(TeachHandwriting.appSplitStrokeAtPoints, InkLimits.SPLIT_AT_POINTS)
        assertEquals(TeachHandwriting.coordScale, InkLimits.SCALE)
        assertEquals(TeachHandwriting.limitXMin * InkLimits.SCALE, InkLimits.X_MIN)
        assertEquals(TeachHandwriting.limitXMax * InkLimits.SCALE, InkLimits.X_MAX)
        assertEquals(TeachHandwriting.limitYMin * InkLimits.SCALE, InkLimits.Y_MIN)
        assertEquals(TeachHandwriting.limitYMax * InkLimits.SCALE, InkLimits.Y_MAX)
        assertEquals(TeachHandwriting.undoDepth, InkLimits.UNDO_DEPTH)
        assertEquals(TeachHandwriting.eraserRadius, InkLimits.ERASER_RADIUS_DP, 0f)
        assertEquals(TeachHandwriting.simplifyEpsilon, InkLimits.SIMPLIFY_EPSILON_DP, 0f)
    }

    @Test
    fun `the widths drawn are the token widths`() {
        assertEquals(TeachHandwriting.widthThin.value, InkWidth.THIN.dp, 0f)
        assertEquals(TeachHandwriting.widthMedium.value, InkWidth.MEDIUM.dp, 0f)
        assertEquals(TeachHandwriting.widthThick.value, InkWidth.THICK.dp, 0f)
        for (w in InkWidth.entries) assertEquals(w.dp, w.penDp(), 0f)
    }
}
