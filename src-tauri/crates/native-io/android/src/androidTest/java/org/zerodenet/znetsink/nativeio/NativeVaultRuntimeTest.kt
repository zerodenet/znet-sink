package org.zerodenet.znetsink.nativeio

import android.content.pm.ApplicationInfo
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore

/** Actual AndroidKeyStore/API24 test app only; no user App, guest, VPN or network. */
@RunWith(AndroidJUnit4::class)
class NativeVaultRuntimeTest {
    @Test fun actualKeystoreWrapsRestoresAndRefusesLostOrCorruptMaterial() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        check(context.packageName.startsWith("org.zerodenet.znetsink.nativeio") && context.packageName.endsWith(".test"))
        check(context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0)
        assertEquals(24, Build.VERSION.SDK_INT)
        val directory = File(context.noBackupFilesDir, "znet-plugin-vault-v1")
        val alias = "org.zerodenet.znetsink.plugin-vault.v1"
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        // Fail on reused test containers rather than erasing unknown material.
        check(!directory.exists() && !store.containsAlias(alias))
        val vault = NativeVault(context)
        var created: ByteArray? = null
        try {
            assertTrue(vault.available())
            assertNull(vault.load(false))
            created = vault.load(true)!!
            assertEquals(32, created.size)
            assertArrayEquals(created, vault.load(false))
            assertNull(store.getKey(alias, null).encoded) // Nonexportable, not a hardware claim.
            val wrapped = File(directory, "master.wrapped")
            val original = wrapped.readBytes()
            assertEquals(NativeVaultEnvelope.BYTES, original.size)
            val changed = original.copyOf().also { it[20] = (it[20].toInt() xor 1).toByte() }
            wrapped.writeBytes(changed)
            try { vault.load(true); fail("corruption must not create a replacement") } catch (_: Exception) { }
            assertArrayEquals(changed, wrapped.readBytes())
            wrapped.writeBytes(original) // Restore only this test's owned fixture.
            assertArrayEquals(created, vault.load(false))
            store.deleteEntry(alias)
            try { vault.load(true); fail("missing native key must not be recreated") } catch (_: Exception) { }
            assertFalse(store.containsAlias(alias))
            assertArrayEquals(original, wrapped.readBytes())
        } finally {
            created?.fill(0)
            store.deleteEntry(alias)
            check(directory.deleteRecursively())
        }
    }
}
