package com.teachme.feature.lesson

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.teachme.core.designsystem.TeachIcons
import com.teachme.core.designsystem.TeachTheme
import com.teachme.core.designsystem.components.TeachIcon
import com.teachme.core.designsystem.components.TeachText
import com.teachme.core.designsystem.components.focusRing
import com.teachme.core.designsystem.components.rememberInteractionState
import com.teachme.core.model.PlaybackSpeed
import com.teachme.sdui.model.Block
import com.teachme.sdui.model.Diagram
import com.teachme.sdui.model.Heading
import com.teachme.sdui.model.Paragraph

/** What the lesson column needs of the audio: the state, and the tap on a block's button. */
@Immutable
class LessonAudioControls(
    val state: LessonAudioUiState,
    val onToggle: (String) -> Unit,
) {
    companion object {
        /** Feature off or lesson without audio: no control anywhere. */
        val Off = LessonAudioControls(LessonAudioUiState()) {}
    }
}

val LocalLessonAudio = compositionLocalOf { LessonAudioControls.Off }

/** Where the control sits and what its accessible name says (lesson-audio.md section 1). */
enum class ListenKind { Paragraph, Heading, Diagram }

/** The kind of control a block gets. A heading has it inline at the end of its line, the others on a row below. */
fun Block.listenKind(): ListenKind =
    when (this) {
        is Heading -> ListenKind.Heading
        is Diagram -> ListenKind.Diagram
        else -> ListenKind.Paragraph
    }

/** True when the block shows its control on a row of its own below it (the gap to the next block gives up 16 dp). */
fun hasControlRowBelow(
    block: Block,
    audio: LessonAudioUiState,
): Boolean = audio.listenState(block.id) != null && block.listenKind() != ListenKind.Heading

/**
 * Wraps [block] with its listening control and, while it is being read, the now-playing mark: one `accent` bar in the
 * margin, outside the text column, no fill (lesson-audio.md sections 1 and 2). A block outside the manifest, or the
 * feature off, draws [content] alone.
 */
@Composable
fun AudioListenable(
    block: Block,
    content: @Composable () -> Unit,
) {
    val audio = LocalLessonAudio.current
    val listen = audio.state.listenState(block.id)
    if (listen == null) {
        content()
        return
    }
    val playing = (listen as? BlockListenState.Playable)?.playing == true
    val kind = block.listenKind()
    val nowPlaying = stringResource(R.string.lesson_audio_now_playing)
    val marked =
        Modifier
            .nowPlayingBar(playing)
            .then(if (playing) Modifier.semantics { stateDescription = nowPlaying } else Modifier)
    if (kind == ListenKind.Heading) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f).then(marked)) { content() }
            ListenButton(listen, kind) { audio.onToggle(block.id) }
        }
    } else {
        Column(verticalArrangement = Arrangement.spacedBy(TeachTheme.spacing.none)) {
            Box(Modifier.fillMaxWidth().then(marked)) { content() }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                ListenButton(listen, kind) { audio.onToggle(block.id) }
            }
        }
    }
}

/** The bar fades in and out in `fast`; with "reduce animations" it is instant. It never moves the content. */
@Composable
private fun Modifier.nowPlayingBar(visible: Boolean): Modifier {
    val reduced = TeachTheme.reducedMotion
    val alpha by animateFloatAsState(
        targetValue = if (visible) 1f else 0f,
        animationSpec = tween(if (reduced) 0 else TeachTheme.motion.fastMs),
        label = "now-playing",
    )
    val color = TeachTheme.colors.accent
    // 4 dp wide (spacing.xxs), 12 dp left of the column (spacing.sm): from -12 to -8 dp.
    val barWidth = TeachTheme.spacing.xxs
    val barOffset = TeachTheme.spacing.sm
    return drawBehind {
        if (alpha > 0f) {
            val width = barWidth.toPx()
            drawRoundRect(
                color = color,
                topLeft = Offset(-barOffset.toPx(), 0f),
                size = Size(width, size.height),
                cornerRadius = CornerRadius(width / 2),
                alpha = alpha,
            )
        }
    }
}

/** The four states of lesson-audio.md section 1: icon only at rest, words for the rare ones. */
@Composable
internal fun ListenButton(
    state: BlockListenState,
    kind: ListenKind,
    onClick: () -> Unit,
) {
    val colors = TeachTheme.colors
    when (state) {
        is BlockListenState.Playable ->
            if (state.playing) {
                AudioControl(
                    icon = TeachIcons.Square,
                    label = stringResource(R.string.lesson_audio_stop),
                    tint = colors.accent,
                    description = stringResource(R.string.lesson_audio_stop_a11y),
                    stateText = stringResource(R.string.lesson_audio_now_playing),
                    onClick = onClick,
                )
            } else {
                AudioControl(
                    icon = TeachIcons.Volume2,
                    label = null,
                    tint = colors.inkSoft,
                    description =
                        stringResource(
                            when (kind) {
                                ListenKind.Paragraph -> R.string.lesson_audio_listen_paragraph_a11y
                                ListenKind.Heading -> R.string.lesson_audio_listen_heading_a11y
                                ListenKind.Diagram -> R.string.lesson_audio_listen_diagram_a11y
                            },
                        ),
                    onClick = onClick,
                )
            }
        BlockListenState.Preparing ->
            AudioControl(
                icon = TeachIcons.Clock,
                label = null,
                tint = colors.inkSoft,
                description = stringResource(R.string.lesson_audio_preparing_a11y),
                enabled = false,
                onClick = {},
            )
        BlockListenState.Unavailable ->
            AudioControl(
                icon = TeachIcons.MinusCircle,
                label = stringResource(R.string.lesson_audio_unavailable),
                tint = colors.inkSoft,
                description = stringResource(R.string.lesson_audio_unavailable_a11y),
                enabled = false,
                onClick = {},
            )
    }
}

/**
 * Icon (20 dp) with an optional word, at least 48 x 48 dp, pressed fill `surfacePressed`, focus ring. Disabled controls
 * keep `inkSoft` (they carry information to read, `disabledInk` is too faint).
 */
@Composable
internal fun AudioControl(
    icon: Int?,
    label: String?,
    tint: Color,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    stateText: String? = null,
) {
    val state = rememberInteractionState()
    val shape = TeachTheme.shapes.full
    val target = TeachTheme.layout.minTouchTarget
    Row(
        modifier
            .sizeIn(minWidth = target, minHeight = target)
            .focusRing(state.focused, shape)
            .background(if (state.pressed && enabled) TeachTheme.colors.surfacePressed else Color.Transparent, shape)
            .clickable(
                interactionSource = state.source,
                indication = null,
                enabled = enabled,
                role = Role.Button,
                onClick = onClick,
            ).padding(horizontal = if (label != null) TeachTheme.spacing.sm else TeachTheme.spacing.none)
            .semantics {
                contentDescription = description
                if (stateText != null) stateDescription = stateText
            },
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon !=
            null
        ) {
            TeachIcon(icon, contentDescription = null, tint = tint, size = TeachTheme.layout.iconSizeSmall)
        }
        if (icon != null && label != null) Spacer(Modifier.width(TeachTheme.spacing.xxs))
        if (label != null) TeachText(label, style = TeachTheme.typography.ui.label, color = tint)
    }
}

/**
 * The audio strip under the top bar while "Ascolta la lezione" runs (lesson-audio.md section 3): "Ferma la lettura" on the
 * left, the speed chip on the right, inside the reading column ([contentPadding]).
 */
@Composable
fun AudioStrip(
    speed: PlaybackSpeed,
    onStop: () -> Unit,
    onCycleSpeed: () -> Unit,
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(horizontal = TeachTheme.spacing.md),
) {
    val colors = TeachTheme.colors
    Column(modifier.fillMaxWidth().background(colors.surface)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = TeachTheme.layout.minTouchTarget)
                .padding(contentPadding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AudioControl(
                icon = TeachIcons.Square,
                label = stringResource(R.string.lesson_audio_stop_reading),
                tint = colors.accent,
                description = stringResource(R.string.lesson_audio_stop_reading),
                onClick = onStop,
            )
            Spacer(Modifier.weight(1f))
            AudioControl(
                icon = null,
                label = speedLabel(speed),
                tint = colors.inkSoft,
                description = speedDescription(speed),
                onClick = onCycleSpeed,
            )
        }
        Box(Modifier.fillMaxWidth().height(TeachTheme.stroke.hairline).background(colors.line))
    }
}

@Composable
internal fun speedLabel(speed: PlaybackSpeed): String =
    stringResource(
        when (speed) {
            PlaybackSpeed.X075 -> R.string.lesson_audio_speed_075
            PlaybackSpeed.X1 -> R.string.lesson_audio_speed_1
            PlaybackSpeed.X125 -> R.string.lesson_audio_speed_125
            PlaybackSpeed.X15 -> R.string.lesson_audio_speed_15
        },
    )

@Composable
private fun speedDescription(speed: PlaybackSpeed): String =
    stringResource(
        when (speed) {
            PlaybackSpeed.X075 -> R.string.lesson_audio_speed_075_a11y
            PlaybackSpeed.X1 -> R.string.lesson_audio_speed_1_a11y
            PlaybackSpeed.X125 -> R.string.lesson_audio_speed_125_a11y
            PlaybackSpeed.X15 -> R.string.lesson_audio_speed_15_a11y
        },
    )

@Composable
private fun ListenSamples(dark: Boolean) {
    TeachTheme(darkTheme = dark) {
        Column(Modifier.background(TeachTheme.colors.bg).padding(16.dp)) {
            for (
            state in
            listOf(
                BlockListenState.Playable("/audio/clips/a", playing = false),
                BlockListenState.Playable("/audio/clips/a", playing = true),
                BlockListenState.Preparing,
                BlockListenState.Unavailable,
            )
            ) {
                ListenButton(state, ListenKind.Paragraph) {}
            }
        }
    }
}

@Preview(name = "Listen buttons, light", showBackground = true)
@Composable
private fun ListenLightPreview() = ListenSamples(dark = false)

@Preview(name = "Listen buttons, dark", showBackground = true)
@Composable
private fun ListenDarkPreview() = ListenSamples(dark = true)

@Composable
private fun NowPlayingSamples(dark: Boolean) {
    val blocks = mapOf("p1" to "Ownership", "p2" to "Sul move", "h1" to "Che cosa accade")

    fun audio(playing: String?) =
        LessonAudioControls(
            LessonAudioUiState(
                available = true,
                blocks =
                    blocks.keys.associateWith {
                        com.teachme.core.model.BlockAudio(
                            it,
                            com.teachme.core.model.AudioStatus.Ready,
                            com.teachme.core.model
                                .AudioClip("/audio/clips/$it", "vivian", 1000),
                        )
                    },
                playingBlockId = playing,
            ),
        ) {}
    TeachTheme(darkTheme = dark) {
        Column(
            Modifier.background(TeachTheme.colors.bg).padding(start = 32.dp, end = 16.dp, top = 16.dp, bottom = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            androidx.compose.runtime.CompositionLocalProvider(LocalLessonAudio provides audio("p1")) {
                AudioListenable(Heading("h1", level = 2, text = "")) {
                    TeachText("Che cosa accade", style = TeachTheme.typography.ui.title3)
                }
                AudioListenable(Paragraph("p1", text = "")) {
                    TeachText(
                        "Il valore passa al nuovo proprietario e il vecchio nome non si può più usare.",
                        style = TeachTheme.typography.reading.body,
                    )
                }
                AudioListenable(Paragraph("p2", text = "")) {
                    TeachText("Un secondo paragrafo, non in lettura.", style = TeachTheme.typography.reading.body)
                }
            }
        }
    }
}

@Preview(name = "Now playing block, light", showBackground = true)
@Composable
private fun NowPlayingLightPreview() = NowPlayingSamples(dark = false)

@Preview(name = "Now playing block, dark", showBackground = true)
@Composable
private fun NowPlayingDarkPreview() = NowPlayingSamples(dark = true)

@Composable
private fun StripSamples(dark: Boolean) {
    TeachTheme(darkTheme = dark) {
        Column(Modifier.background(TeachTheme.colors.bg)) {
            AudioStrip(PlaybackSpeed.X125, onStop = {}, onCycleSpeed = {})
            Spacer(Modifier.height(16.dp))
            AudioStrip(PlaybackSpeed.X075, onStop = {}, onCycleSpeed = {})
        }
    }
}

@Preview(name = "Audio strip, light", showBackground = true)
@Composable
private fun StripLightPreview() = StripSamples(dark = false)

@Preview(name = "Audio strip, dark", showBackground = true)
@Composable
private fun StripDarkPreview() = StripSamples(dark = true)

@Preview(name = "Audio strip, font 2.0", showBackground = true, fontScale = 2f)
@Composable
private fun StripLargeFontPreview() = StripSamples(dark = false)
