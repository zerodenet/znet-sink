package org.zerodenet.znetsink.nativeio

import org.junit.Assert.*
import org.junit.Test
import javax.crypto.KeyGenerator

class NativeVaultEnvelopeTest {
    @Test fun realEnvelopeIsBoundToWrappingKeyAndApplicationIdentity() {
        val generator = KeyGenerator.getInstance("AES").apply { init(256) }
        val key = generator.generateKey()
        val master = ByteArray(32) { (it + 1).toByte() }
        val aad = "example.debug/plugin-vault-master/v1".toByteArray()
        val envelope = NativeVaultEnvelope.seal(key, master, aad)
        assertEquals(NativeVaultEnvelope.BYTES, envelope.size)
        assertArrayEquals(master, NativeVaultEnvelope.open(key, envelope, aad))
        for (attempt in listOf(
            { NativeVaultEnvelope.open(key, envelope, "other.application".toByteArray()) },
            { NativeVaultEnvelope.open(generator.generateKey(), envelope, aad) },
            { NativeVaultEnvelope.open(key, envelope.copyOf().also { it[20] = (it[20].toInt() xor 1).toByte() }, aad) },
            { NativeVaultEnvelope.open(key, envelope.copyOf(envelope.size + 1), aad) },
            { NativeVaultEnvelope.open(key, byteArrayOf(1), aad) },
        )) {
            try { attempt(); fail("invalid envelope must not be accepted") } catch (_: Exception) { }
        }
        assertFalse(master.contentEquals(envelope))
    }
}
