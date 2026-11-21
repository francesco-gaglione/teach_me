package com.teachme.core.designsystem.components

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import com.teachme.core.designsystem.LocalContentColor
import com.teachme.core.designsystem.LocalTextStyle

/** Text with a theme style. Color defaults to the current content color. */
@Composable
fun TeachText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = LocalTextStyle.current,
    color: Color = Color.Unspecified,
    textAlign: TextAlign = TextAlign.Start,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
) {
    TeachText(AnnotatedString(text), modifier, style, color, textAlign, maxLines, overflow)
}

@Composable
fun TeachText(
    text: AnnotatedString,
    modifier: Modifier = Modifier,
    style: TextStyle = LocalTextStyle.current,
    color: Color = Color.Unspecified,
    textAlign: TextAlign = TextAlign.Start,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
) {
    val resolved = if (color != Color.Unspecified) color else LocalContentColor.current
    BasicText(
        text = text,
        modifier = modifier,
        style = style.merge(TextStyle(color = resolved, textAlign = textAlign)),
        maxLines = maxLines,
        overflow = overflow,
    )
}

/** Sets content color and text style for the children. */
@Composable
fun ProvideContent(
    color: Color,
    style: TextStyle = LocalTextStyle.current,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(
        LocalContentColor provides color,
        LocalTextStyle provides style,
        content = content,
    )
}
