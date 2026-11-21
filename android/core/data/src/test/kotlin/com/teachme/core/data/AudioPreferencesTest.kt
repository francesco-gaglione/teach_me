package com.teachme.core.data

import com.teachme.core.model.PlaybackSpeed
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

internal class AudioPreferencesTest : DataStoreTest() {
    @Test
    fun `the speed is 1x until the user changes it`() =
        runTest {
            assertEquals(PlaybackSpeed.X1, DataStoreAudioPreferences(store("settings")).speed.first())
        }

    @Test
    fun `the chosen speed comes back after a restart`() =
        runTest {
            DataStoreAudioPreferences(store("settings")).setSpeed(PlaybackSpeed.X125)
            killProcess()
            assertEquals(PlaybackSpeed.X125, DataStoreAudioPreferences(store("settings")).speed.first())
        }

    @Test
    fun `an unknown stored value reads as 1x`() {
        assertEquals(PlaybackSpeed.X1, PlaybackSpeed.fromWire("2.5"))
        assertEquals(PlaybackSpeed.X075, PlaybackSpeed.fromWire("0.75"))
        assertEquals(PlaybackSpeed.X075, PlaybackSpeed.X15.next())
    }
}
