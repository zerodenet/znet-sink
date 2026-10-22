package org.zerodenet.znetsink.nativeio

import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

class NativeAppUpdatePolicyTest {
    private val installed = NativeAppUpdatePolicy.Facts("org.zerodenet.znetsink.debug", "1.0.0", 10, 24, setOf("certificate-a"), setOf("x86"))
    private val update = installed.copy(version = "1.1.0", code = 11)
    private fun reject(candidate: NativeAppUpdatePolicy.Facts) = NativeAppUpdatePolicy.rejection(installed, candidate, "1.1.0", 24, setOf("x86"))
    @Test fun actualApkIdentityAndDeviceFactsAreRequired() {
        assertNull(reject(update))
        assertEquals("package-mismatch", reject(update.copy(packageName = "org.zerodenet.znetsink")))
        assertEquals("version-not-newer", reject(update.copy(code = 10)))
        assertEquals("version-name-mismatch", reject(update.copy(version = "1.2.0")))
        assertEquals("sdk-incompatible", reject(update.copy(minSdk = 25)))
        assertEquals("abi-incompatible", reject(update.copy(abis = setOf("arm64-v8a"))))
        assertNull(reject(update.copy(abis = emptySet()))) // Java-only APK is architecture independent.
    }
    @Test fun certificatesMustMatchExactlyAndNoRotationIsGuessed() {
        assertEquals("signer-mismatch", reject(update.copy(signers = emptySet())))
        assertEquals("signer-mismatch", reject(update.copy(signers = setOf("certificate-b"))))
        assertEquals("signer-mismatch", reject(update.copy(signers = setOf("certificate-a", "certificate-b"))))
        assertFalse(NativeAppUpdatePolicy.confirms(installed, update))
        assertTrue(NativeAppUpdatePolicy.confirms(update, update))
        assertFalse(NativeAppUpdatePolicy.confirms(update.copy(signers = emptySet()), update))
        assertFalse(NativeAppUpdatePolicy.confirms(update.copy(code = 12), update))
    }
    @Test fun catalogAndRedirectOriginsAndBudgetsAreClosed() {
        val url = "https://github.com/zerodenet/znet-sink/releases/download/v1.1.0/client-x86.apk"
        assertTrue(NativeAppUpdatePolicy.asset(url, "client-x86.apk", "1.1.0", 100))
        for (bad in listOf(url.replace("https:", "http:"), url.replace("github.com", "github.com.evil.test"), "$url?token=secret", url.replace("znet-sink", "core"), url.replace("client-x86.apk", "../client-x86.apk"))) {
            assertFalse(bad, NativeAppUpdatePolicy.asset(bad, "client-x86.apk", "1.1.0", 100))
        }
        assertFalse(NativeAppUpdatePolicy.asset(url, "client-x86.apk", "1.1.0", NativeAppUpdatePolicy.MAX_BYTES + 1))
        assertTrue(NativeAppUpdatePolicy.redirect("https://release-assets.githubusercontent.com/release.apk?sig=temporary"))
        for (bad in listOf("file:///data/a.apk", "https://github.com:444/a.apk", "https://user@github.com/a.apk", "https://evil.test/a.apk", "http://github.com/a.apk")) assertFalse(bad, NativeAppUpdatePolicy.redirect(bad))
        assertTrue(NativeAppUpdatePolicy.operation("e2695293-5580-4eec-9437-a1e6b2621111"))
        assertFalse(NativeAppUpdatePolicy.operation("../../settings"))
        assertFalse(NativeAppUpdatePolicy.digest("A".repeat(64)))
    }
    @Test fun realStreamNeverWritesPastExpectedSizeAndRejectsTruncation() {
        val out = ByteArrayOutputStream()
        assertEquals(3L, NativeAppUpdatePolicy.transfer(ByteArrayInputStream(byteArrayOf(1, 2, 3)), out, 3, {}, {}, { 0 }))
        assertArrayEquals(byteArrayOf(1, 2, 3), out.toByteArray())
        for ((bytes, size, reason) in listOf(Triple(byteArrayOf(1, 2, 3, 4), 3L, "download-size-budget"), Triple(byteArrayOf(1), 3L, "download-truncated"))) {
            val target = ByteArrayOutputStream()
            try { NativeAppUpdatePolicy.transfer(ByteArrayInputStream(bytes), target, size, {}, {}, { 0 }); fail("must reject invalid byte count") }
            catch (error: IllegalStateException) { assertEquals(reason, error.message) }
            assertTrue(target.size() <= size)
        }
    }
    @Test fun cancelledAndExpiredTransferCannotContinueWriting() {
        var activeChecks = 0
        val output = ByteArrayOutputStream()
        try {
            NativeAppUpdatePolicy.transfer(ByteArrayInputStream(ByteArray(100)), output, 100,
                { check(++activeChecks < 2) { "cancelled" } }, {}, { 0 })
            fail("must cancel after read before write")
        } catch (error: IllegalStateException) { assertEquals("cancelled", error.message) }
        assertEquals(0, output.size())
        try {
            NativeAppUpdatePolicy.transfer(ByteArrayInputStream(ByteArray(100)), output, 100, {}, {}, { 601L * 1_000_000_000 })
            fail("must stop expired stream")
        } catch (error: IllegalStateException) { assertEquals("download-attempt-timeout", error.message) }
        assertEquals(0, output.size())
    }
}
