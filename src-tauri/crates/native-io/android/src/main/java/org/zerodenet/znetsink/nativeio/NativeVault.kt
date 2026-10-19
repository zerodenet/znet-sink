package org.zerodenet.znetsink.nativeio

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.io.RandomAccessFile
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Fixed binary envelope; this helper is exercised with real JVM crypto too. */
internal object NativeVaultEnvelope {
    const val BYTES = 1 + 12 + 32 + 16
    fun seal(key: SecretKey, master: ByteArray, aad: ByteArray): ByteArray {
        require(master.size == 32)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(aad)
        require(cipher.iv.size == 12)
        return byteArrayOf(1) + cipher.iv + cipher.doFinal(master)
    }
    fun open(key: SecretKey, envelope: ByteArray, aad: ByteArray): ByteArray {
        require(envelope.size == BYTES && envelope[0] == 1.toByte())
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, envelope.copyOfRange(1, 13)))
        cipher.updateAAD(aad)
        val master = cipher.doFinal(envelope, 13, envelope.size - 13)
        require(master.size == 32)
        return master
    }
}

/** No exported key, raw master file, user-chosen path, or backup of wrapped material. */
internal class NativeVault(private val context: Context) {
    private val alias = "org.zerodenet.znetsink.plugin-vault.v1"
    private val aad get() = "${context.packageName}/plugin-vault-master/v1".toByteArray(Charsets.UTF_8)
    fun available(): Boolean = try {
        KeyStore.getInstance("AndroidKeyStore").apply { load(null) }; true
    } catch (_: Exception) { false }

    @Synchronized fun load(create: Boolean): ByteArray? {
        val directory = File(context.noBackupFilesDir, "znet-plugin-vault-v1")
        if (!directory.exists() && !create) return null
        check(directory.isDirectory || directory.mkdirs())
        // Cross-process creation must not replace a different wrapping/key identity.
        RandomAccessFile(File(directory, "coordination.lock"), "rw").use { lockFile ->
            lockFile.channel.lock().use {
                val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                val file = AtomicFile(File(directory, "master.wrapped"))
                val hasEnvelope = file.baseFile.exists() || File(directory, "master.wrapped.bak").exists()
                val existing = store.getKey(alias, null)
                if (hasEnvelope) {
                    check(existing is SecretKey) // Missing key is not an empty vault.
                    val bytes = file.openRead().use { NativeIoPolicy.readBounded(it, NativeVaultEnvelope.BYTES.toLong()) }
                    return NativeVaultEnvelope.open(existing, bytes, aad)
                }
                if (!create) return null
                val wrapping = if (existing != null) {
                    check(existing is SecretKey); existing
                } else {
                    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
                    generator.init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                        .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setRandomizedEncryptionRequired(true).setUserAuthenticationRequired(false).build())
                    generator.generateKey()
                }
                val master = ByteArray(32).also { SecureRandom().nextBytes(it) }
                try {
                    val wrapped = NativeVaultEnvelope.seal(wrapping, master, aad)
                    val output = file.startWrite()
                    try { output.write(wrapped); file.finishWrite(output) }
                    catch (failure: Exception) { file.failWrite(output); throw failure }
                    // Return authoritative re-read material, not a successful write assumption.
                    val stored = file.openRead().use { NativeIoPolicy.readBounded(it, NativeVaultEnvelope.BYTES.toLong()) }
                    return NativeVaultEnvelope.open(wrapping, stored, aad)
                } finally { master.fill(0) }
            }
        }
    }
}
