package com.teachme.core.designsystem

import android.provider.Settings
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight

internal val LocalTeachColors = staticCompositionLocalOf { LightTeachColors }
internal val LocalTeachTypography =
    staticCompositionLocalOf<TeachTypography> {
        error("TeachTheme not provided")
    }
internal val LocalTeachSpacing = staticCompositionLocalOf { TeachSpacing() }
internal val LocalTeachLayout = staticCompositionLocalOf { TeachLayout() }
internal val LocalTeachShapes = staticCompositionLocalOf { TeachShapes() }
internal val LocalTeachStroke = staticCompositionLocalOf { TeachStroke() }
internal val LocalTeachElevation = staticCompositionLocalOf { TeachElevation() }
internal val LocalTeachMotion = staticCompositionLocalOf { TeachMotion() }
internal val LocalReducedMotion = staticCompositionLocalOf { false }
val LocalIsDark = staticCompositionLocalOf { false }

/** Color applied to [TeachText] and [TeachIcon] when none is given. */
val LocalContentColor = staticCompositionLocalOf { Color.Unspecified }

/** Style applied to [TeachText] when none is given. */
val LocalTextStyle = staticCompositionLocalOf { TextStyle.Default }

/** Access to the design tokens of the current [TeachTheme]. */
object TeachTheme {
    val colors: TeachColors
        @Composable @ReadOnlyComposable
        get() = LocalTeachColors.current
    val typography: TeachTypography
        @Composable @ReadOnlyComposable
        get() = LocalTeachTypography.current
    val spacing: TeachSpacing
        @Composable @ReadOnlyComposable
        get() = LocalTeachSpacing.current
    val layout: TeachLayout
        @Composable @ReadOnlyComposable
        get() = LocalTeachLayout.current
    val shapes: TeachShapes
        @Composable @ReadOnlyComposable
        get() = LocalTeachShapes.current
    val stroke: TeachStroke
        @Composable @ReadOnlyComposable
        get() = LocalTeachStroke.current
    val elevation: TeachElevation
        @Composable @ReadOnlyComposable
        get() = LocalTeachElevation.current
    val motion: TeachMotion
        @Composable @ReadOnlyComposable
        get() = LocalTeachMotion.current

    /** True when the user turned animations off in system settings: switch instantly. */
    val reducedMotion: Boolean
        @Composable @ReadOnlyComposable
        get() = LocalReducedMotion.current
}

/**
 * Variable fonts bundled in res/font (OFL). A Font per weight selects the axis value; the system
 * families are the fallback if loading fails.
 */
@Composable
private fun rememberFonts(): TeachFonts =
    remember {
        fun family(
            resId: Int,
            fallback: FontFamily,
            weights: List<Int>,
        ): FontFamily =
            runCatching {
                FontFamily(
                    weights.map { w ->
                        Font(
                            resId = resId,
                            weight = FontWeight(w),
                            variationSettings = FontVariation.Settings(FontVariation.weight(w)),
                        )
                    },
                )
            }.getOrDefault(fallback)
        TeachFonts(
            ui = family(R.font.inter, FontFamily.SansSerif, listOf(400, 600, 700)),
            reading = family(R.font.literata, FontFamily.Serif, listOf(400, 500, 600)),
            code = family(R.font.jetbrains_mono, FontFamily.Monospace, listOf(400, 600)),
        )
    }

@Composable
private fun rememberReducedMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return runCatching {
        Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }.getOrDefault(false)
}

/**
 * The app theme: custom design system on `androidx.compose.foundation`, no Material, no dynamic color.
 */
@Composable
fun TeachTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = if (darkTheme) DarkTeachColors else LightTeachColors
    val fonts = rememberFonts()
    val typography = remember(fonts) { teachTypography(fonts) }
    CompositionLocalProvider(
        LocalTeachColors provides colors,
        LocalTeachTypography provides typography,
        LocalTeachSpacing provides remember { TeachSpacing() },
        LocalTeachLayout provides remember { TeachLayout() },
        LocalTeachShapes provides remember { TeachShapes() },
        LocalTeachStroke provides remember { TeachStroke() },
        LocalTeachElevation provides remember { TeachElevation() },
        LocalTeachMotion provides remember { TeachMotion() },
        LocalReducedMotion provides rememberReducedMotion(),
        LocalIsDark provides darkTheme,
        LocalContentColor provides colors.ink,
        LocalTextStyle provides typography.ui.bodyL,
        LocalTextSelectionColors provides
            TextSelectionColors(
                handleColor = colors.accent,
                backgroundColor = colors.accentSoft,
            ),
        content = content,
    )
}
