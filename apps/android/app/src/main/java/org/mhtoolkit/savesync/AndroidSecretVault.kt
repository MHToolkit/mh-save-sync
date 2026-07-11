package org.mhtoolkit.savesync

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.spec.GCMParameterSpec

internal object RecoverySecretFormat {
    fun decodeHex(value: String): ByteArray {
        val clean = value.trim()
        require(clean.length == 64 && clean.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) { "恢复密钥必须是 64 位十六进制" }
        return clean.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    }
}

class AndroidSecretVault(private val context: Context) {
    private val prefs = context.getSharedPreferences("mh_save_sync_secret", Context.MODE_PRIVATE)
    fun hasSecret() = prefs.contains("wrapped") && prefs.contains("iv")
    fun store(secret: ByteArray) {
        require(secret.size == 32)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val encrypted = cipher.doFinal(secret)
        check(
            prefs.edit().putString("wrapped", android.util.Base64.encodeToString(encrypted, android.util.Base64.NO_WRAP))
                .putString("iv", android.util.Base64.encodeToString(cipher.iv, android.util.Base64.NO_WRAP)).commit()
        ) { "无法安全保存恢复密钥" }
    }
    fun load(): ByteArray {
        val encrypted = android.util.Base64.decode(requireNotNull(prefs.getString("wrapped", null)), android.util.Base64.NO_WRAP)
        val iv = android.util.Base64.decode(requireNotNull(prefs.getString("iv", null)), android.util.Base64.NO_WRAP)
        return Cipher.getInstance("AES/GCM/NoPadding").run { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv)); doFinal(encrypted) }
    }
    private fun key(): java.security.Key {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        store.getKey("mh-save-sync-account-wrap-v1", null)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder("mh-save-sync-account-wrap-v1", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setKeySize(256).build())
            generateKey()
        }
    }
}
