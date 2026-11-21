package com.teachme.core.designsystem

import androidx.compose.animation.core.CubicBezierEasing
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Guards GeneratedTokens.kt against drift from docs/design/tokens.json (the source of truth).
 * If this fails, run `python3 -I android/tools/generate_tokens.py` and commit the result.
 */
class TokensMatchJsonTest {
    private val tokens: JsonObject = Json.parseToJsonElement(findTokensFile().readText()).jsonObject

    private fun findTokensFile(): File {
        var dir: File? = File(System.getProperty("user.dir")).absoluteFile
        while (dir != null) {
            val candidate = File(dir, "docs/design/tokens.json")
            if (candidate.isFile) return candidate
            dir = dir.parentFile
        }
        error("docs/design/tokens.json not found above ${System.getProperty("user.dir")}")
    }

    private fun obj(vararg path: String): JsonObject = path.fold(tokens) { acc, key -> acc.getValue(key).jsonObject }

    private fun argb(hex: String): Int = (0xFF000000L or hex.removePrefix("#").toLong(16)).toInt()

    @Test
    fun `version is the one the generator was written for`() {
        assertEquals(2, tokens.getValue("version").jsonPrimitive.int)
    }

    @Test
    fun `light and dark colors match every role`() {
        for ((mode, actual) in listOf("light" to LightTeachColors, "dark" to DarkTeachColors)) {
            val expected = obj("color", mode).mapValues { argb(it.value.jsonPrimitive.content) }
            assertEquals("roles in $mode", expected.keys, actual.asMap().keys)
            assertEquals("values in $mode", expected, actual.asMap())
        }
    }

    @Test
    fun `overlay opacity matches`() {
        assertEquals(obj("colorMeta").getValue("overlayOpacity").jsonPrimitive.float, OVERLAY_OPACITY, 0f)
    }

    private fun assertDpGroup(
        key: String,
        actual: Map<String, Float>,
    ) {
        val expected = obj(key).mapValues { it.value.jsonPrimitive.float }
        assertEquals(key, expected, actual)
    }

    @Test
    fun `spacing layout stroke and shape radii match`() {
        assertDpGroup("spacing", TeachSpacing().asMap())
        assertDpGroup("layout", TeachLayout().asMap())
        assertDpGroup("stroke", TeachStroke().asMap())
        assertDpGroup("shape", TeachRadii().asMap())
    }

    @Test
    fun `typography styles match`() {
        val fonts =
            com.teachme.core.designsystem.TeachFonts(
                androidx.compose.ui.text.font.FontFamily.SansSerif,
                androidx.compose.ui.text.font.FontFamily.Serif,
                androidx.compose.ui.text.font.FontFamily.Monospace,
            )
        val actual = teachTypography(fonts).asMap()
        val expected =
            listOf("ui", "reading", "code")
                .flatMap { group ->
                    obj("typography", group).map { (name, spec) -> "$group.$name" to spec.jsonObject }
                }.toMap()
        assertEquals(expected.keys, actual.keys)
        for ((name, spec) in expected) {
            val style = actual.getValue(name)
            assertEquals("$name size", spec.getValue("fontSize").jsonPrimitive.float, style.fontSize.value, 0f)
            assertEquals("$name line", spec.getValue("lineHeight").jsonPrimitive.float, style.lineHeight.value, 0f)
            assertEquals(
                "$name tracking",
                spec.getValue("letterSpacing").jsonPrimitive.float,
                style.letterSpacing.value,
                0f,
            )
            assertEquals("$name weight", spec.getValue("fontWeight").jsonPrimitive.int, style.fontWeight?.weight)
            val isCode = spec.getValue("fontFamily").jsonPrimitive.content == "code"
            assertEquals("$name ligatures off", isCode, style.fontFeatureSettings == "liga 0, calt 0")
        }
    }

    @Test
    fun `font family names are the ones bundled`() {
        val expected =
            obj("typography", "fontFamilies").mapValues {
                it.value.jsonObject
                    .getValue("name")
                    .jsonPrimitive.content
            }
        assertEquals(expected, FontFamilyNames)
    }

    @Test
    fun `motion durations and easings match`() {
        val motion = TeachMotion()
        val durations =
            obj("motion", "duration").mapValues { it.value.jsonPrimitive.int } +
                obj("motion", "semantic").mapValues {
                    it.value.jsonObject
                        .getValue("duration")
                        .jsonPrimitive.int
                }
        assertEquals(durations, motion.durationsMap())

        fun easing(name: String): CubicBezierEasing {
            val p = (obj("motion", "easing").getValue(name) as JsonArray).map { (it as JsonPrimitive).float }
            return CubicBezierEasing(p[0], p[1], p[2], p[3])
        }
        assertEquals(easing("out"), motion.easeOut)
        assertEquals(easing("inOut"), motion.easeInOut)
        assertEquals(easing("in_"), motion.easeIn)
        assertEquals(easing("linear"), motion.linear)
        // Semantic easings reference the named ones.
        for ((name, spec) in obj("motion", "semantic")) {
            val expected =
                easing(
                    spec.jsonObject
                        .getValue("easing")
                        .jsonPrimitive.content,
                )
            val actual =
                when (name) {
                    "press" -> motion.press
                    "feedbackAppear" -> motion.feedbackAppear
                    "selectionChange" -> motion.selectionChange
                    "sectionReveal" -> motion.sectionReveal
                    "screenEnter" -> motion.screenEnter
                    "screenExit" -> motion.screenExit
                    "cardReveal" -> motion.cardReveal
                    "progress" -> motion.progress
                    "unlock" -> motion.unlock
                    else -> error("semantic motion '$name' missing from the test: add it here")
                }
            assertEquals("$name easing", expected, actual.easing)
        }
    }

    @Test
    fun `elevation shadows match`() {
        val e = TeachElevation()
        val raised = obj("elevation", "raised")
        assertEquals(raised.getValue("y").jsonPrimitive.float, e.raised.y.value, 0f)
        assertEquals(raised.getValue("blur").jsonPrimitive.float, e.raised.blur.value, 0f)
        assertEquals(raised.getValue("alphaLight").jsonPrimitive.float, e.raised.alphaLight, 0f)
        assertEquals(raised.getValue("alphaDark").jsonPrimitive.float, e.raised.alphaDark, 0f)
        val overlay = obj("elevation", "overlay")
        assertEquals(overlay.getValue("blur").jsonPrimitive.float, e.overlay.blur.value, 0f)
        assertEquals(overlay.getValue("alphaDark").jsonPrimitive.float, e.overlay.alphaDark, 0f)
        assertTrue(obj("elevation").keys == setOf("flat", "raised", "overlay"))
    }

    @Test
    fun `handwriting measures match`() {
        val hw = obj("handwriting")

        fun f(vararg path: String): Float =
            path
                .dropLast(
                    1,
                ).fold(hw) { acc, key -> acc.getValue(key).jsonObject }
                .getValue(path.last())
                .jsonPrimitive.float

        fun i(vararg path: String): Int = f(*path).toInt()
        assertEquals(f("eraserGhostAlpha"), TeachHandwriting.eraserGhostAlpha, 0f)
        assertEquals(f("eraserRadius"), TeachHandwriting.eraserRadius, 0f)
        assertEquals(f("simplifyEpsilon"), TeachHandwriting.simplifyEpsilon, 0f)
        assertEquals(i("coordScale"), TeachHandwriting.coordScale)
        assertEquals(i("appSplitStrokeAtPoints"), TeachHandwriting.appSplitStrokeAtPoints)
        assertEquals(i("undoDepth"), TeachHandwriting.undoDepth)
        assertEquals(i("syncDebounceMs"), TeachHandwriting.syncDebounceMs)
        assertEquals(f("widths", "thin"), TeachHandwriting.widthThin.value, 0f)
        assertEquals(f("widths", "medium"), TeachHandwriting.widthMedium.value, 0f)
        assertEquals(f("widths", "thick"), TeachHandwriting.widthThick.value, 0f)
        val palette =
            mapOf(
                "toolButton" to TeachHandwriting.paletteToolButton,
                "toolIcon" to TeachHandwriting.paletteToolIcon,
                "separatorWidth" to TeachHandwriting.paletteSeparatorWidth,
                "separatorGap" to TeachHandwriting.paletteSeparatorGap,
                "widthSampleLength" to TeachHandwriting.paletteWidthSampleLength,
                "widthSwatch" to TeachHandwriting.paletteWidthSwatch,
                "undoRedoButton" to TeachHandwriting.paletteUndoRedoButton,
                "collapsedBelowWindowWidth" to TeachHandwriting.paletteCollapsedBelowWindowWidth,
                "popoverGap" to TeachHandwriting.palettePopoverGap,
                "selectedBarHeight" to TeachHandwriting.paletteSelectedBarHeight,
                "selectedBarWidth" to TeachHandwriting.paletteSelectedBarWidth,
            )
        assertEquals("every palette key is generated", obj("handwriting", "palette").keys, palette.keys)
        for ((name, actual) in palette) assertEquals("palette.$name", f("palette", name), actual.value, 0f)
        assertEquals(i("limits", "maxStrokes"), TeachHandwriting.limitMaxStrokes)
        assertEquals(i("limits", "maxPointsPerStroke"), TeachHandwriting.limitMaxPointsPerStroke)
        assertEquals(i("limits", "maxPointsPerLesson"), TeachHandwriting.limitMaxPointsPerLesson)
        assertEquals(i("limits", "x", "min"), TeachHandwriting.limitXMin)
        assertEquals(i("limits", "x", "max"), TeachHandwriting.limitXMax)
        assertEquals(i("limits", "y", "min"), TeachHandwriting.limitYMin)
        assertEquals(i("limits", "y", "max"), TeachHandwriting.limitYMax)
        assertEquals(i("limits", "maxBodyBytes"), TeachHandwriting.limitMaxBodyBytes)
    }
}
