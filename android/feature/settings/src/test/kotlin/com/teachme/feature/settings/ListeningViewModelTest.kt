package com.teachme.feature.settings

import androidx.lifecycle.viewModelScope
import com.teachme.core.model.ApiErrorDetail
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioManifest
import com.teachme.core.model.AudioPreferences
import com.teachme.core.model.AudioRepository
import com.teachme.core.model.ClipPlayer
import com.teachme.core.model.ClipPlayerFactory
import com.teachme.core.model.PlaybackEnd
import com.teachme.core.model.PlaybackSpeed
import com.teachme.core.model.TtsVoice
import com.teachme.core.model.TtsVoices
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ListeningViewModelTest {
    private val voices =
        listOf(
            TtsVoice("vivian", "Vivian", "/audio/clips/a"),
            TtsVoice("serena", "Serena", null),
            TtsVoice("aiden", "Aiden", "/audio/clips/c"),
        )

    private class FakeAudio(
        var voices: suspend () -> ApiResult<TtsVoices>,
    ) : AudioRepository {
        var calls = 0

        override suspend fun getManifest(lessonId: String): ApiResult<AudioManifest> = error("not used")

        override suspend fun getVoices(): ApiResult<TtsVoices> {
            calls++
            return voices()
        }
    }

    private class FakePrefs : AudioPreferences {
        val state = MutableStateFlow(PlaybackSpeed.Default)
        override val speed = state

        override suspend fun setSpeed(speed: PlaybackSpeed) {
            state.value = speed
        }
    }

    private class FakePlayer : ClipPlayer {
        val played = mutableListOf<String>()
        val speeds = mutableListOf<Float>()
        var stops = 0
        var released = false
        private var onEnded: ((PlaybackEnd) -> Unit)? = null

        override fun play(
            clipUrl: String,
            onEnded: (PlaybackEnd) -> Unit,
        ) {
            played += clipUrl
            this.onEnded = onEnded
        }

        override fun setSpeed(factor: Float) {
            speeds += factor
        }

        override fun stop() {
            stops++
            onEnded = null
        }

        override fun release() {
            released = true
        }

        fun end(end: PlaybackEnd) = onEnded?.invoke(end)
    }

    private val sessions = FakeSessions(USER.copy(ttsVoice = "vivian"))
    private val auth = FakeAuth()
    private val prefs = FakePrefs()
    private val player = FakePlayer()
    private var creations = 0
    private val audio = FakeAudio({ on() })
    private val created = mutableListOf<ListeningViewModel>()

    private fun on(
        list: List<TtsVoice> = voices,
        default: String? = "vivian",
    ): ApiResult<TtsVoices> = ApiResult.Success(TtsVoices(true, default, list))

    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun test(body: suspend TestScope.(ListeningViewModel) -> Unit) =
        runTest {
            val vm =
                ListeningViewModel(audio, prefs, auth, sessions, ClipPlayerFactory { player.also { creations++ } })
            created += vm
            try {
                body(vm)
            } finally {
                created.forEach { it.viewModelScope.cancel() }
            }
        }

    private fun TestScope.start(vm: ListeningViewModel) {
        vm.onStarted()
        runCurrent()
    }

    // region Visibility

    @Test
    fun `the section is hidden until the list arrives and shows the voices in order`() =
        test { vm ->
            assertFalse(vm.state.value.visible)
            start(vm)
            assertTrue(vm.state.value.visible)
            assertEquals(
                listOf("vivian", "serena", "aiden"),
                vm.state.value.voices
                    .map { it.id },
            )
            assertEquals("vivian", vm.state.value.selectedVoice)
        }

    @Test
    fun `the section is hidden when the feature is off`() =
        test { vm ->
            audio.voices = { ApiResult.Success(TtsVoices(false, null, emptyList())) }
            start(vm)
            assertFalse(vm.state.value.visible)
            assertTrue(
                vm.state.value.voices
                    .isEmpty(),
            )
        }

    @Test
    fun `the section is hidden when the request fails, and comes back at the next start`() =
        test { vm ->
            audio.voices = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            start(vm)
            assertFalse(vm.state.value.visible)

            audio.voices = { on() }
            start(vm)
            assertTrue(vm.state.value.visible)
            assertEquals(2, audio.calls)
        }

    @Test
    fun `a stored voice that is not in the list shows the default one`() =
        test { vm ->
            audio.voices = { on(default = "aiden") }
            sessions.updateUser(USER.copy(ttsVoice = "gone"))
            start(vm)
            assertEquals("aiden", vm.state.value.selectedVoice)
        }

    // endregion

    // region Speed

    @Test
    fun `speed follows the stored preference and a tap saves it`() =
        test { vm ->
            runCurrent()
            assertEquals(PlaybackSpeed.X1, vm.state.value.speed)

            vm.onSpeedSelected(PlaybackSpeed.X125)
            runCurrent()
            assertEquals(PlaybackSpeed.X125, prefs.state.value)
            assertEquals(PlaybackSpeed.X125, vm.state.value.speed)

            // Changed elsewhere (the lesson chip): the section follows.
            prefs.state.value = PlaybackSpeed.X075
            runCurrent()
            assertEquals(PlaybackSpeed.X075, vm.state.value.speed)
        }

    @Test
    fun `the speed row stacks from font scale 1,3`() {
        assertFalse(speedStacked(1f))
        assertFalse(speedStacked(1.29f))
        assertTrue(speedStacked(1.3f))
        assertTrue(speedStacked(2f))
    }

    // endregion

    // region Voice

    @Test
    fun `a tap moves the radio at once, saves with PUT and stores the answer`() =
        test { vm ->
            start(vm)
            val gate = CompletableDeferred<Unit>()
            auth.setVoice = {
                gate.await()
                ApiResult.Success(USER.copy(ttsVoice = it))
            }

            vm.onVoiceSelected("aiden")
            runCurrent()
            assertEquals("aiden", vm.state.value.selectedVoice)
            assertEquals("aiden", vm.state.value.savingVoice)

            gate.complete(Unit)
            runCurrent()
            assertEquals(listOf("aiden"), auth.sentVoices)
            assertNull(vm.state.value.savingVoice)
            assertEquals("aiden", vm.state.value.selectedVoice)
            assertEquals(
                "aiden",
                sessions.updates
                    .single()
                    .ttsVoice,
            )
        }

    @Test
    fun `the current voice and taps during a save are ignored`() =
        test { vm ->
            start(vm)
            vm.onVoiceSelected("vivian")
            runCurrent()
            assertTrue(auth.sentVoices.isEmpty())

            val gate = CompletableDeferred<Unit>()
            auth.setVoice = {
                gate.await()
                ApiResult.Success(USER.copy(ttsVoice = it))
            }
            vm.onVoiceSelected("aiden")
            runCurrent()
            vm.onVoiceSelected("serena")
            runCurrent()
            gate.complete(Unit)
            runCurrent()
            assertEquals(listOf("aiden"), auth.sentVoices)
        }

    private fun TestScope.rolledBack(vm: ListeningViewModel) {
        assertEquals("vivian", vm.state.value.selectedVoice)
        assertNull(vm.state.value.savingVoice)
        assertTrue(sessions.updates.isEmpty())
    }

    @Test
    fun `422 unknown_voice rolls back, tells the user and asks the list again`() =
        test { vm ->
            val before = audio.calls
            val validation = ApiFailure.Validation(listOf(ApiErrorDetail("voice", "unknown_voice")))
            start(vm)
            auth.setVoice = { ApiResult.Failure(validation) }
            vm.onVoiceSelected("aiden")
            runCurrent()
            rolledBack(vm)
            assertEquals(ListeningEffect.VoiceSaveFailed, takeEffect(vm))
            assertEquals(before + 2, audio.calls)
        }

    @Test
    fun `409 rolls back and the reloaded list may hide the section`() =
        test { vm ->
            start(vm)
            auth.setVoice = { ApiResult.Failure(ApiFailure.Conflict) }
            audio.voices = { ApiResult.Success(TtsVoices(false, null, emptyList())) }
            vm.onVoiceSelected("aiden")
            runCurrent()
            assertEquals(ListeningEffect.VoiceSaveFailed, takeEffect(vm))
            assertFalse(vm.state.value.visible)
        }

    @Test
    fun `a network error rolls back and tells the user`() =
        test { vm ->
            start(vm)
            auth.setVoice = { ApiResult.Failure(ApiFailure.Network(UnreachableReason.Timeout)) }
            vm.onVoiceSelected("aiden")
            runCurrent()
            rolledBack(vm)
            assertEquals(ListeningEffect.VoiceSaveFailed, takeEffect(vm))
        }

    @Test
    fun `a 401 rolls back silently`() =
        test { vm ->
            start(vm)
            auth.setVoice = { ApiResult.Failure(ApiFailure.Unauthorized) }
            vm.onVoiceSelected("aiden")
            runCurrent()
            rolledBack(vm)
            assertNull(takeEffect(vm))
        }

    private suspend fun takeEffect(vm: ListeningViewModel): ListeningEffect? =
        kotlinx.coroutines.withTimeoutOrNull(1) { vm.effects.first() }

    // endregion

    // region Samples

    @Test
    fun `sample states follow sample_url and a missing clip is Preparing`() =
        test { vm ->
            start(vm)
            assertEquals(
                listOf(SampleState.Ready, SampleState.Preparing, SampleState.Ready),
                vm.state.value.voices
                    .map { it.sample },
            )
            vm.onSampleClicked("serena")
            assertTrue(player.played.isEmpty())
            assertEquals(0, creations)
        }

    @Test
    fun `a sample plays at 1x even when the reading speed is 1,5x, and does not select the voice`() =
        test { vm ->
            prefs.state.value = PlaybackSpeed.X15
            start(vm)
            vm.onSampleClicked("aiden")

            assertEquals(listOf("/audio/clips/c"), player.played)
            assertEquals(listOf(1f), player.speeds)
            assertEquals(
                SampleState.Playing,
                vm.state.value.voices[2]
                    .sample,
            )
            assertEquals("vivian", vm.state.value.selectedVoice)
            assertTrue(auth.sentVoices.isEmpty())
        }

    @Test
    fun `only one sample plays at a time and the same button stops it`() =
        test { vm ->
            start(vm)
            vm.onSampleClicked("vivian")
            vm.onSampleClicked("aiden")
            assertEquals(listOf("/audio/clips/a", "/audio/clips/c"), player.played)
            assertEquals(
                listOf(SampleState.Ready, SampleState.Preparing, SampleState.Playing),
                vm.state.value.voices
                    .map { it.sample },
            )
            assertEquals(1, creations)

            vm.onSampleClicked("aiden")
            assertEquals(
                SampleState.Ready,
                vm.state.value.voices[2]
                    .sample,
            )
            assertEquals(1, player.stops)
        }

    @Test
    fun `a clip that ends goes back to Prova, one that fails becomes Non disponibile until the next start`() =
        test { vm ->
            start(vm)
            vm.onSampleClicked("vivian")
            player.end(PlaybackEnd.Completed)
            assertEquals(
                SampleState.Ready,
                vm.state.value.voices[0]
                    .sample,
            )

            vm.onSampleClicked("vivian")
            player.end(PlaybackEnd.Error)
            assertEquals(
                SampleState.Unavailable,
                vm.state.value.voices[0]
                    .sample,
            )
            vm.onSampleClicked("vivian")
            assertEquals(2, player.played.size)

            start(vm)
            assertEquals(
                SampleState.Ready,
                vm.state.value.voices[0]
                    .sample,
            )
        }

    @Test
    fun `leaving the screen stops the sample and clearing the model releases the player`() =
        test { vm ->
            start(vm)
            vm.onSampleClicked("vivian")
            vm.onStopped()
            assertEquals(
                SampleState.Ready,
                vm.state.value.voices[0]
                    .sample,
            )
            assertEquals(1, player.stops)

            assertFalse(player.released)
            ListeningViewModel::class.java
                .getDeclaredMethod("onCleared")
                .apply { isAccessible = true }
                .invoke(vm)
            assertTrue(player.released)
        }

    // endregion
}
