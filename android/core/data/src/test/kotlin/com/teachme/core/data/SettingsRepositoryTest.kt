package com.teachme.core.data

import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.teachme.core.model.BackendSettings
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

internal class SettingsRepositoryTest : DataStoreTest() {
    @Test
    fun `the token reaches the disk wrapped, never as it is`() =
        runTest {
            val ds = store("settings")
            DataStoreSettingsRepository(
                ds,
                ReversingCipher(),
            ).update(BackendSettings("http://teach.test", "tm_plain-token", "u1"))

            val raw =
                ds.data
                    .first()
                    .asMap()
                    .values
                    .map { it.toString() }
            assertFalse(raw.any { it.contains("tm_plain-token") })
            assertTrue(raw.any { it.contains("enc:") })
        }

    @Test
    fun `the session comes back after a restart`() =
        runTest {
            DataStoreSettingsRepository(
                store("settings"),
                ReversingCipher(),
            ).update(BackendSettings("http://teach.test", "tm_t", "u1"))
            killProcess()

            val stored = DataStoreSettingsRepository(store("settings"), ReversingCipher()).current()

            assertEquals(BackendSettings("http://teach.test", "tm_t", "u1"), stored)
        }

    @Test
    fun `the old plain static token is never read, and is gone from the disk after the first read`() =
        runTest {
            val ds = store("settings")
            ds.edit {
                it[stringPreferencesKey("base_url")] = "http://teach.test"
                it[stringPreferencesKey("token")] = "the-old-static-token"
            }
            val repo = DataStoreSettingsRepository(ds, ReversingCipher())

            assertEquals("", repo.current().token)
            assertEquals("http://teach.test", repo.current().baseUrl)

            assertNull(ds.data.first()[stringPreferencesKey("token")])
        }

    @Test
    fun `a token that cannot be read after a restore is no session, and the user id goes with it`() =
        runTest {
            val ds = store("settings")
            DataStoreSettingsRepository(
                ds,
                ReversingCipher(),
            ).update(BackendSettings("http://teach.test", "tm_t", "u1"))

            // The key is gone (data restored on another device): the cipher cannot read the stored text.
            val lost =
                object : TokenCipher {
                    override fun encrypt(plain: String) = plain

                    override fun decrypt(stored: String): String? = null
                }

            val stored = DataStoreSettingsRepository(ds, lost).current()

            assertEquals(BackendSettings("http://teach.test", "", ""), stored)
        }

    @Test
    fun `clearing the token keeps the address`() =
        runTest {
            val repo = DataStoreSettingsRepository(store("settings"), ReversingCipher())
            repo.update(BackendSettings("http://teach.test", "t", "u1"))

            repo.update(BackendSettings("http://teach.test"))

            assertEquals(BackendSettings("http://teach.test"), repo.current())
        }

    @Test
    fun `the Keystore is not asked at every request`() =
        runTest {
            var decrypts = 0
            val counting =
                object : TokenCipher {
                    override fun encrypt(plain: String) = ReversingCipher().encrypt(plain)

                    override fun decrypt(stored: String): String? {
                        decrypts++
                        return ReversingCipher().decrypt(stored)
                    }
                }
            val repo = DataStoreSettingsRepository(store("settings"), counting)
            repo.update(BackendSettings("http://teach.test", "tm_t", "u1"))

            repeat(20) { assertEquals("tm_t", repo.current().token) }
            assertEquals(0, decrypts)

            // Another process start (a fresh object) reads and decrypts once, then keeps it.
            killProcess()
            val reopened = DataStoreSettingsRepository(store("settings"), counting)
            repeat(20) { reopened.current() }
            assertEquals(1, decrypts)
        }

    @Test
    fun `a new login and a sign out replace what is kept in memory`() =
        runTest {
            val repo = DataStoreSettingsRepository(store("settings"), ReversingCipher())
            repo.update(BackendSettings("http://teach.test", "tm_1", "u1"))
            assertEquals("tm_1", repo.current().token)

            repo.update(BackendSettings("http://teach.test", "tm_2", "u2"))
            assertEquals(BackendSettings("http://teach.test", "tm_2", "u2"), repo.current())
            repo.update(BackendSettings("http://teach.test"))
            assertEquals("", repo.current().token)
        }
}
