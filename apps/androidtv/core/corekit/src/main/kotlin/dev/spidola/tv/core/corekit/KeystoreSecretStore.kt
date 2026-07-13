// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import uniffi.core_api.SecretStore
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The host-secrets callback (TECH_SPEC §12): the core stores only opaque keys in SQLite and
 * calls back here to read or write the actual secret. Values are sealed under an AES-GCM key
 * held in the Android Keystore (hardware-backed where the device offers it) and persisted as
 * ciphertext in a private preferences file — plaintext secrets never touch disk or logs.
 *
 * UniFFI may call these methods from any core thread; access is serialized.
 */
class KeystoreSecretStore(context: Context) : SecretStore {
    private val prefs =
        context.applicationContext.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)
    private val lock = Any()

    override fun get(key: String): String? =
        synchronized(lock) {
            val stored = prefs.getString(key, null) ?: return@synchronized null
            try {
                decrypt(stored)
            } catch (e: GeneralSecurityException) {
                Log.w(TAG, "stored secret could not be unsealed; treating as absent", e)
                null
            }
        }

    override fun set(
        key: String,
        value: String,
    ) {
        synchronized(lock) {
            try {
                prefs.edit().putString(key, encrypt(value)).apply()
            } catch (e: GeneralSecurityException) {
                Log.e(TAG, "secret could not be sealed; not persisted", e)
            }
        }
    }

    override fun delete(key: String) {
        synchronized(lock) {
            prefs.edit().remove(key).apply()
        }
    }

    private fun encrypt(plain: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val iv = cipher.iv
        val ciphertext = cipher.doFinal(plain.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(iv + ciphertext, Base64.NO_WRAP)
    }

    private fun decrypt(stored: String): String {
        val blob = Base64.decode(stored, Base64.NO_WRAP)
        val iv = blob.copyOfRange(0, GCM_IV_BYTES)
        val ciphertext = blob.copyOfRange(GCM_IV_BYTES, blob.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
        return String(cipher.doFinal(ciphertext), Charsets.UTF_8)
    }

    private fun secretKey(): SecretKey {
        val keystore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keystore.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec
                .Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val TAG = "spidola::secrets"
        const val PREFS_FILE = "spidola_secrets"
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "dev.spidola.tv.secrets"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val GCM_IV_BYTES = 12
        const val GCM_TAG_BITS = 128
    }
}
