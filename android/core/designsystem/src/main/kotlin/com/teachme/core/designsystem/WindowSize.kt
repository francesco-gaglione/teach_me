package com.teachme.core.designsystem

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo

/**
 * Width classes of the window (not of the device: multi-window and rotation change it). Breakpoints
 * follow `tokens.json` layout.compactMaxWidth / mediumMaxWidth. Computed from the Compose window
 * size, so no extra dependency is needed and nothing from Material is pulled in.
 */
enum class WindowWidthClass { Compact, Medium, Expanded }

@Composable
fun currentWindowWidthClass(): WindowWidthClass {
    val widthDp =
        with(LocalDensity.current) {
            LocalWindowInfo.current.containerSize.width
                .toDp()
        }
    val layout = TeachTheme.layout
    return when {
        widthDp < layout.compactMaxWidth -> WindowWidthClass.Compact
        widthDp < layout.mediumMaxWidth -> WindowWidthClass.Medium
        else -> WindowWidthClass.Expanded
    }
}

/** Horizontal screen margin for the width class (design-system.md section 5.1). */
@Composable
fun screenMargin(widthClass: WindowWidthClass = currentWindowWidthClass()): androidx.compose.ui.unit.Dp =
    when (widthClass) {
        WindowWidthClass.Compact -> TeachTheme.layout.screenMarginCompact
        WindowWidthClass.Medium -> TeachTheme.layout.screenMarginMedium
        WindowWidthClass.Expanded -> TeachTheme.layout.screenMarginExpanded
    }
