package com.teachme.feature.lesson

import androidx.lifecycle.SavedStateHandle
import com.teachme.core.model.ApiFailure
import com.teachme.core.model.ApiResult
import com.teachme.core.model.AudioClip
import com.teachme.core.model.AudioManifest
import com.teachme.core.model.AudioPreferences
import com.teachme.core.model.AudioRepository
import com.teachme.core.model.AudioStatus
import com.teachme.core.model.BlockAudio
import com.teachme.core.model.ClipPlayer
import com.teachme.core.model.PlaybackEnd
import com.teachme.core.model.PlaybackSpeed
import com.teachme.core.model.TtsVoices
import com.teachme.core.model.UnreachableReason
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
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
class LessonAudioViewModelTest {
    @Before
    fun setUp() = Dispatchers.setMain(StandardTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private class FakeAudio(
        var available: Boolean = true,
        manifests: List<AudioManifest>,
    ) : AudioRepository {
        private val queue = manifests.toMutableList()
        var manifestCalls = 0
        var voicesCalls = 0

        /** Failures answered first, one per call, before the normal answers. */
        val manifestFailures = ArrayDeque<ApiFailure>()
        val voicesFailures = ArrayDeque<ApiFailure>()

        override suspend fun getManifest(lessonId: String): ApiResult<AudioManifest> {
            manifestCalls++
            manifestFailures.removeFirstOrNull()?.let { return ApiResult.Failure(it) }
            return ApiResult.Success(if (queue.size > 1) queue.removeAt(0) else queue.first())
        }

        override suspend fun getVoices(): ApiResult<TtsVoices> {
            voicesCalls++
            voicesFailures.removeFirstOrNull()?.let { return ApiResult.Failure(it) }
            return ApiResult.Success(TtsVoices(available, "vivian", emptyList()))
        }
    }

    private class FakePlayer : ClipPlayer {
        val played = mutableListOf<String>()
        var stops = 0
        var released = false
        var onEnded: ((PlaybackEnd) -> Unit)? = null
        val speeds = mutableListOf<Float>()

        override fun setSpeed(factor: Float) {
            speeds += factor
        }

        override fun play(
            clipUrl: String,
            onEnded: (PlaybackEnd) -> Unit,
        ) {
            played += clipUrl
            this.onEnded = onEnded
        }

        override fun stop() {
            stops++
        }

        override fun release() {
            released = true
        }
    }

    private class FakePrefs(
        initial: PlaybackSpeed = PlaybackSpeed.Default,
    ) : AudioPreferences {
        val stored = MutableStateFlow(initial)
        override val speed: Flow<PlaybackSpeed> = stored

        var saveDelayMs = 0L

        override suspend fun setSpeed(speed: PlaybackSpeed) {
            if (saveDelayMs > 0) delay(saveDelayMs)
            stored.value = speed
        }
    }

    private val created = mutableListOf<LessonAudioViewModel>()

    /** A pending lesson polls forever: close every model before [runTest] drains the scheduler. */
    private fun audioTest(block: suspend TestScope.() -> Unit) =
        runTest {
            try {
                block()
            } finally {
                created.forEach { it.clearForTest() }
            }
        }

    private fun clip(
        id: String,
        voice: String = "vivian",
    ) = AudioClip("/audio/clips/$id", voice, 1000)

    private fun manifest(vararg blocks: BlockAudio) = AudioManifest("vivian", blocks.toList())

    private val ready = BlockAudio("p1", AudioStatus.Ready, clip("a"))
    private val ready2 = BlockAudio("p2", AudioStatus.Ready, clip("b"))
    private val pending = BlockAudio("p3", AudioStatus.Pending, null)
    private val failed = BlockAudio("d1", AudioStatus.Failed, null)
    private val fallback = BlockAudio("c1", AudioStatus.Pending, clip("c", voice = "serena"))

    private fun TestScope.vm(
        audio: AudioRepository,
        player: FakePlayer = FakePlayer(),
        key: String = "L1",
        prefs: AudioPreferences = FakePrefs(),
    ): LessonAudioViewModel {
        val model =
            LessonAudioViewModel(
                SavedStateHandle(mapOf("key" to key)),
                audio,
                { player },
                object : AudioTimings() {
                    override val manifestPollMs = 15_000L
                },
                prefs,
            )
        created += model
        runCurrent()
        return model
    }

    @Test
    fun buttonStatesFollowTheManifest() =
        audioTest {
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, pending, failed, fallback))))
            val s = model.state.value
            assertEquals(BlockListenState.Playable("/audio/clips/a", false), s.listenState("p1"))
            assertEquals(BlockListenState.Preparing, s.listenState("p3"))
            assertEquals(BlockListenState.Unavailable, s.listenState("d1"))
            // Another voice's clip is played while the new one is not ready.
            assertEquals(BlockListenState.Playable("/audio/clips/c", false), s.listenState("c1"))
            // Not in the manifest: no control.
            assertNull(s.listenState("img"))
        }

    @Test
    fun featureOffHidesEverythingAndNeverAsksTheManifest() =
        audioTest {
            val audio = FakeAudio(available = false, manifests = listOf(manifest(ready)))
            val model = vm(audio)
            advanceTimeBy(60_000)
            assertFalse(model.state.value.available)
            assertNull(model.state.value.listenState("p1"))
            assertEquals(0, audio.manifestCalls)
        }

    @Test
    fun pollsEvery15sWhileAnyBlockIsPendingThenStops() =
        audioTest {
            val audio =
                FakeAudio(
                    manifests =
                        listOf(
                            manifest(ready, pending),
                            manifest(ready, pending),
                            manifest(ready, BlockAudio("p3", AudioStatus.Ready, clip("z"))),
                        ),
                )
            val model = vm(audio)
            assertEquals(1, audio.manifestCalls)
            advanceTimeBy(14_999)
            assertEquals(1, audio.manifestCalls)
            advanceTimeBy(1)
            runCurrent()
            assertEquals(2, audio.manifestCalls)
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(3, audio.manifestCalls)
            assertEquals(BlockListenState.Playable("/audio/clips/z", false), model.state.value.listenState("p3"))
            advanceTimeBy(120_000)
            assertEquals(3, audio.manifestCalls)
        }

    @Test
    fun pollingStopsWhenTheLessonCloses() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(pending)))
            val model = vm(audio)
            advanceTimeBy(15_000)
            runCurrent()
            val calls = audio.manifestCalls
            assertTrue(calls >= 2)
            model.clearForTest()
            advanceTimeBy(60_000)
            assertEquals(calls, audio.manifestCalls)
        }

    @Test
    fun sampleLessonsAreSkipped() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(ready)))
            vm(audio, key = "fixture/x")
            advanceTimeBy(20_000)
            assertEquals(0, audio.manifestCalls)
        }

    @Test
    fun toggleStartsMarksPlayingAndStopsOnSecondTap() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.toggle("p1")
            assertEquals(listOf("/audio/clips/a"), player.played)
            assertEquals("p1", model.state.value.playingBlockId)
            assertEquals(BlockListenState.Playable("/audio/clips/a", true), model.state.value.listenState("p1"))
            model.toggle("p1")
            assertNull(model.state.value.playingBlockId)
            assertEquals(1, player.stops)
        }

    @Test
    fun startingAnotherBlockReplacesTheFirst() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.toggle("p1")
            model.toggle("p2")
            assertEquals(listOf("/audio/clips/a", "/audio/clips/b"), player.played)
            assertEquals("p2", model.state.value.playingBlockId)
        }

    @Test
    fun disabledButtonsDoNothing() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(pending, failed))), player)
            model.toggle("p3")
            model.toggle("d1")
            assertTrue(player.played.isEmpty())
            assertNull(model.state.value.playingBlockId)
        }

    @Test
    fun clipEndClearsTheNowPlayingMark() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), player)
            model.toggle("p1")
            player.onEnded?.invoke(PlaybackEnd.Completed)
            assertNull(model.state.value.playingBlockId)
        }

    @Test
    fun backgroundStopsReading() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), player)
            model.toggle("p1")
            model.stop()
            assertNull(model.state.value.playingBlockId)
            assertEquals(1, player.stops)
        }

    @Test
    fun leavingTheLessonReleasesThePlayer() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), player)
            model.toggle("p1")
            model.clearForTest()
            assertTrue(player.released)
        }

    // ---- Continuous reading ("Ascolta la lezione") ----

    private val p3 = BlockAudio("p3", AudioStatus.Ready, clip("c"))
    private val p4 = BlockAudio("p4", AudioStatus.Ready, clip("d"))

    private fun slots(vararg slot: ReadingSlot): () -> List<ReadingSlot> = { slot.toList() }

    private fun read(vararg ids: String) = ids.map { ReadingSlot.Read(it) }.toTypedArray()

    private fun FakePlayer.finishClip() = onEnded?.invoke(PlaybackEnd.Completed)

    @Test
    fun continuousReadingFollowsDocumentOrderAndSkipsBlocksWithoutClip() =
        audioTest {
            val player = FakePlayer()
            // p2 is in the manifest without a clip, "img" is not in the manifest at all.
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, pending.copy(blockId = "p2"), p3))), player)
            val events = mutableListOf<LessonAudioEvent>()
            val job = launch(Dispatchers.Unconfined) { model.events.toList(events) }
            model.listenToLesson("p1", slots(*read("p1", "p2", "img", "p3")))
            assertTrue(model.state.value.continuous)
            assertEquals(listOf("/audio/clips/a"), player.played)
            player.finishClip()
            assertEquals(listOf("/audio/clips/a", "/audio/clips/c"), player.played)
            assertEquals("p3", model.state.value.playingBlockId)
            player.finishClip()
            // Last block read: the reading ends by itself.
            assertFalse(model.state.value.continuous)
            assertNull(model.state.value.playingBlockId)
            assertEquals(
                listOf(
                    LessonAudioEvent.Reading("p1"),
                    LessonAudioEvent.Reading("p3"),
                    LessonAudioEvent.Finished,
                ),
                events,
            )
            job.cancel()
        }

    @Test
    fun readingStartsFromTheGivenBlockNotFromTheTop() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2, p3))), player)
            model.listenToLesson("p2", slots(*read("p1", "p2", "p3")))
            assertEquals(listOf("/audio/clips/b"), player.played)
        }

    @Test
    fun readingStopsBeforeARequiredUnsolvedExerciseAndNeverReadsIt() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2, p3))), player)
            val events = mutableListOf<LessonAudioEvent>()
            val job = launch(Dispatchers.Unconfined) { model.events.toList(events) }
            model.listenToLesson(
                "p1",
                slots(ReadingSlot.Read("p1"), ReadingSlot.Read("p2"), ReadingSlot.Gate("q1"), ReadingSlot.Read("p3")),
            )
            player.finishClip()
            assertEquals(listOf("/audio/clips/a", "/audio/clips/b"), player.played)
            // The audio of p2 ends: it does not start the exercise, nor what follows it.
            player.finishClip()
            assertEquals(2, player.played.size)
            assertFalse(model.state.value.continuous)
            assertNull(model.state.value.playingBlockId)
            assertEquals(LessonAudioEvent.StoppedAtExercise("q1"), events.last())
            assertFalse(events.contains(LessonAudioEvent.Finished))
            job.cancel()
        }

    @Test
    fun answeringAnExerciseWhileReadingLetsTheReadingGoOn() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, p3))), player)
            var answered = false
            model.listenToLesson("p1") {
                listOf(
                    ReadingSlot.Read("p1"),
                    if (answered) ReadingSlot.Read("q1") else ReadingSlot.Gate("q1"),
                    ReadingSlot.Read("p3"),
                )
            }
            answered = true
            player.finishClip()
            assertEquals(listOf("/audio/clips/a", "/audio/clips/c"), player.played)
        }

    @Test
    fun startingRightBeforeAGateStopsAtOnce() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), player)
            val events = mutableListOf<LessonAudioEvent>()
            val job = launch(Dispatchers.Unconfined) { model.events.toList(events) }
            model.listenToLesson("q1", slots(ReadingSlot.Gate("q1"), ReadingSlot.Read("p1")))
            assertTrue(player.played.isEmpty())
            assertFalse(model.state.value.continuous)
            assertEquals(listOf<LessonAudioEvent>(LessonAudioEvent.StoppedAtExercise("q1")), events)
            job.cancel()
        }

    @Test
    fun aFailingClipEndsTheContinuousReading() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.listenToLesson("p1", slots(*read("p1", "p2")))
            player.onEnded?.invoke(PlaybackEnd.Error)
            assertEquals(1, player.played.size)
            assertNull(model.state.value.playingBlockId)
            assertFalse(model.state.value.continuous)
        }

    @Test
    fun theLessonControlIsDisabledUntilABlockIsReady() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(pending, failed))), player)
            assertTrue(model.state.value.available)
            assertFalse(model.state.value.canListenToLesson)
            model.listenToLesson("p3", slots(*read("p3")))
            assertTrue(player.played.isEmpty())
            assertFalse(model.state.value.continuous)
            val ok = vm(FakeAudio(manifests = listOf(manifest(pending, ready))), player)
            assertTrue(ok.state.value.canListenToLesson)
        }

    @Test
    fun startingABlockOrStoppingEndsTheContinuousReading() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2, p3))), player)
            model.listenToLesson("p1", slots(*read("p1", "p2", "p3")))
            // Another block: the continuous reading is replaced and does not move on afterwards.
            model.toggle("p3")
            assertFalse(model.state.value.continuous)
            assertEquals("p3", model.state.value.playingBlockId)
            player.finishClip()
            assertEquals(listOf("/audio/clips/a", "/audio/clips/c"), player.played)
            assertNull(model.state.value.playingBlockId)

            model.listenToLesson("p1", slots(*read("p1", "p2")))
            model.stop()
            assertFalse(model.state.value.continuous)
            assertNull(model.state.value.playingBlockId)
            // The old callback must not restart anything.
            player.finishClip()
            assertEquals(3, player.played.size)
        }

    @Test
    fun startingTheLessonReplacesABlockInProgress() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.toggle("p1")
            model.listenToLesson("p2", slots(*read("p1", "p2")))
            assertEquals("p2", model.state.value.playingBlockId)
            assertTrue(model.state.value.continuous)
        }

    @Test
    fun backgroundAndLeavingStillStopTheContinuousReading() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.listenToLesson("p1", slots(*read("p1", "p2")))
            model.stop()
            assertFalse(model.state.value.continuous)
            assertEquals(1, player.stops)
            model.listenToLesson("p1", slots(*read("p1", "p2")))
            model.clearForTest()
            assertTrue(player.released)
        }

    // ---- Speed ----

    @Test
    fun speedStartsFromTheStoredValue() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), player, prefs = FakePrefs(PlaybackSpeed.X15))
            assertEquals(PlaybackSpeed.X15, model.state.value.speed)
            // The player is created with it.
            model.toggle("p1")
            assertEquals(1.5f, player.speeds.last(), 0f)
        }

    @Test
    fun theSpeedChipCyclesAndTheValueIsStored() =
        audioTest {
            val prefs = FakePrefs()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), prefs = prefs)
            val seen = mutableListOf<PlaybackSpeed>()
            repeat(5) {
                model.cycleSpeed()
                runCurrent()
                seen += model.state.value.speed
                assertEquals(model.state.value.speed, prefs.stored.value)
            }
            assertEquals(
                listOf(PlaybackSpeed.X125, PlaybackSpeed.X15, PlaybackSpeed.X075, PlaybackSpeed.X1, PlaybackSpeed.X125),
                seen,
            )
        }

    @Test
    fun changingTheSpeedWhilePlayingReachesThePlayerAtOnce() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            model.listenToLesson("p1", slots(*read("p1", "p2")))
            model.cycleSpeed()
            assertEquals(1.25f, player.speeds.last(), 0f)
            // The next clip keeps the new speed (it is a property of the player).
            player.finishClip()
            model.cycleSpeed()
            assertEquals(1.5f, player.speeds.last(), 0f)
            runCurrent()
            assertEquals(PlaybackSpeed.X15, model.state.value.speed)
        }

    private val httpError = ApiFailure.Unexpected(500)

    @Test
    fun aServerErrorOrRateLimitDuringThePollDoesNotStopIt() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(ready, pending)))
            val model = vm(audio)
            assertEquals(1, audio.manifestCalls)
            audio.manifestFailures += httpError
            audio.manifestFailures += ApiFailure.RateLimited(40)
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(2, audio.manifestCalls)
            // The 500 waits the normal interval, the 429 waits Retry-After (40 s) when it is longer.
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(3, audio.manifestCalls)
            advanceTimeBy(39_999)
            runCurrent()
            assertEquals(3, audio.manifestCalls)
            advanceTimeBy(1)
            runCurrent()
            assertEquals(4, audio.manifestCalls)
            assertEquals(BlockListenState.Preparing, model.state.value.listenState("p3"))
        }

    @Test
    fun aShortRetryAfterStillWaitsAtLeast15Seconds() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(ready, pending)))
            vm(audio)
            audio.manifestFailures += ApiFailure.RateLimited(1)
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(2, audio.manifestCalls)
            advanceTimeBy(14_999)
            runCurrent()
            assertEquals(2, audio.manifestCalls)
            advanceTimeBy(1)
            runCurrent()
            assertEquals(3, audio.manifestCalls)
        }

    @Test
    fun theVoicesCallIsRetriedOnServerErrorAndRateLimit() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(ready)))
            audio.voicesFailures += httpError
            audio.voicesFailures += ApiFailure.RateLimited(null)
            audio.voicesFailures += ApiFailure.Network(UnreachableReason.Timeout)
            val model = vm(audio)
            assertEquals(1, audio.voicesCalls)
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(2, audio.voicesCalls)
            advanceTimeBy(15_000)
            runCurrent()
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(4, audio.voicesCalls)
            assertEquals(1, audio.manifestCalls)
            assertTrue(model.state.value.available)
        }

    @Test
    fun failuresARetryCannotFixStopThePolling() =
        audioTest {
            for (failure in listOf(ApiFailure.Unauthorized, ApiFailure.NotFound, ApiFailure.InvalidId)) {
                val audio = FakeAudio(manifests = listOf(manifest(pending)))
                audio.manifestFailures += failure
                vm(audio)
                advanceTimeBy(120_000)
                assertEquals(failure.toString(), 1, audio.manifestCalls)
                val voices = FakeAudio(manifests = listOf(manifest(pending)))
                voices.voicesFailures += failure
                vm(voices)
                advanceTimeBy(120_000)
                assertEquals(failure.toString(), 1, voices.voicesCalls)
            }
        }

    @Test
    fun pollingStopsOnStopAndResumesOnStart() =
        audioTest {
            val audio = FakeAudio(manifests = listOf(manifest(pending)))
            val model = vm(audio)
            model.onStopped()
            val calls = audio.manifestCalls
            advanceTimeBy(120_000)
            runCurrent()
            assertEquals(calls, audio.manifestCalls)
            model.onStarted()
            runCurrent()
            assertEquals(calls + 1, audio.manifestCalls)
            advanceTimeBy(15_000)
            runCurrent()
            assertEquals(calls + 2, audio.manifestCalls)
            // A second start while polling does not duplicate the loop.
            model.onStarted()
            runCurrent()
            assertEquals(calls + 2, audio.manifestCalls)
        }

    @Test
    fun fastSpeedTapsDoNotFlickerWhenTheStoredValueArrivesLate() =
        audioTest {
            val prefs = FakePrefs().apply { saveDelayMs = 100 }
            val model = vm(FakeAudio(manifests = listOf(manifest(ready))), prefs = prefs)
            val seen = mutableListOf<PlaybackSpeed>()
            val job = launch { model.state.collect { seen += it.speed } }
            runCurrent()
            model.cycleSpeed()
            advanceTimeBy(50)
            model.cycleSpeed()
            advanceTimeBy(50)
            model.cycleSpeed()
            advanceTimeBy(500)
            runCurrent()
            assertEquals(PlaybackSpeed.X075, model.state.value.speed)
            assertEquals(PlaybackSpeed.X075, prefs.stored.value)
            assertEquals(listOf(PlaybackSpeed.X1, PlaybackSpeed.X125, PlaybackSpeed.X15, PlaybackSpeed.X075), seen)
            job.cancel()
        }

    @Test
    fun aPlaybackErrorShowsAToastEventForASingleClipAndForTheContinuousReading() =
        audioTest {
            val player = FakePlayer()
            val model = vm(FakeAudio(manifests = listOf(manifest(ready, ready2))), player)
            val events = mutableListOf<LessonAudioEvent>()
            val job = launch { model.events.collect { events += it } }
            runCurrent()
            model.toggle("p1")
            player.onEnded?.invoke(PlaybackEnd.Error)
            runCurrent()
            assertEquals(listOf<LessonAudioEvent>(LessonAudioEvent.PlaybackFailed), events)
            events.clear()
            model.listenToLesson("p1", slots(*read("p1", "p2")))
            runCurrent()
            events.clear()
            player.onEnded?.invoke(PlaybackEnd.Error)
            runCurrent()
            assertEquals(listOf<LessonAudioEvent>(LessonAudioEvent.PlaybackFailed), events)
            job.cancel()
        }
}
