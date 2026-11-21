package com.teachme.core.data

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Protects the session token at rest. The token is a bearer secret: with the key in the Android Keystore (not
 * extractable, bound to this app and this device) the file in DataStore is useless on its own, for example in a
 * device copy that restores app data without the Keystore.
 */
interface TokenCipher {
    fun encrypt(plain: String): String

    /** Null when [stored] cannot be read (key gone after a restore or a screen-lock reset): the session is then lost. */
    fun decrypt(stored: String): String?
}

/** AES-256-GCM with a key generated in the Android Keystore. */
class KeystoreTokenCipher : TokenCipher {
    /** Synchronized: two first calls must not both generate the key. */
    @Synchronized
    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec
                .Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(KEY_BITS)
                .build(),
        )
        return generator.generateKey()
    }

    private fun wrap(plain: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key()) }
        val body = cipher.doFinal(plain.toByteArray(Charsets.UTF_8))
        return "$PREFIX_KEYSTORE${b64(cipher.iv)}:${b64(body)}"
    }

    /** A broken or stale key (an alias that survived a reinstall, say): drop it, so the next [key] makes a new one. */
    @Synchronized
    private fun resetKey() {
        KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }.deleteEntry(ALIAS)
    }

    override fun encrypt(plain: String): String =
        try {
            try {
                wrap(plain)
            } catch (e: Exception) {
                resetKey()
                wrap(plain)
            }
        } catch (e: Exception) {
            // A device whose Keystore does not work must still be usable: the token then stays as plain as it was
            // before Phase 11 (private app storage, backup off). Only the failure type is logged, never the value.
            Log.w(TAG, "keystore unavailable, token stored without wrapping (${e::class.simpleName})")
            "$PREFIX_PLAIN$plain"
        }

    override fun decrypt(stored: String): String? =
        try {
            when {
                stored.startsWith(PREFIX_PLAIN) -> stored.removePrefix(PREFIX_PLAIN)
                stored.startsWith(PREFIX_KEYSTORE) -> {
                    val (iv, body) = stored.removePrefix(PREFIX_KEYSTORE).split(':', limit = 2)
                    val cipher =
                        Cipher.getInstance(TRANSFORMATION).apply {
                            init(
                                Cipher.DECRYPT_MODE,
                                key(),
                                GCMParameterSpec(TAG_BITS, Base64.decode(iv, Base64.NO_WRAP)),
                            )
                        }
                    String(cipher.doFinal(Base64.decode(body, Base64.NO_WRAP)), Charsets.UTF_8)
                }
                else -> null
            }
        } catch (e: Exception) {
            Log.w(TAG, "cannot read the stored session (${e::class.simpleName})")
            null
        }

    private fun b64(bytes: ByteArray) = Base64.encodeToString(bytes, Base64.NO_WRAP)

    private companion object {
        const val TAG = "TokenCipher"
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "teachme_session_token"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val KEY_BITS = 256
        const val TAG_BITS = 128
        const val PREFIX_KEYSTORE = "k1:"
        const val PREFIX_PLAIN = "p1:"
    }
}
