package com.teachme.core.designsystem

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape

/** The `raised` shadow of the design system (popovers, floating pills): blur used as elevation, black alpha per theme. */
@Composable
fun Modifier.raisedShadow(shape: Shape): Modifier {
    val shadow = TeachTheme.elevation.raised
    val alpha = if (LocalIsDark.current) shadow.alphaDark else shadow.alphaLight
    return shadow(shadow.blur, shape, ambientColor = Color.Black.copy(alpha), spotColor = Color.Black.copy(alpha))
}
