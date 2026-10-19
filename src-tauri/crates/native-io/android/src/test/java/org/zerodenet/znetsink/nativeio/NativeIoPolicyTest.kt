package org.zerodenet.znetsink.nativeio

import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.InputStream

class NativeIoPolicyTest {
    private fun validate(
        action: String = "save", purpose: String = "settings-export", budget: Long = 4,
        name: String? = "settings.json", mime: String = "application/json", body: String? = "e30=",
        acknowledged: Boolean = false,
    ) = NativeIoPolicy.validate(action, purpose, budget, name, mime, body, acknowledged)

    @Test fun operationsAreBoundAndUrlSafe() {
        assertTrue(NativeIoPolicy.validOperation("operation_1-abc"))
        assertTrue(NativeIoPolicy.validOperation("a".repeat(64)))
        for (value in listOf("", "a".repeat(65), "../secret", "token\n", "空")) {
            assertFalse(NativeIoPolicy.validOperation(value))
        }
    }

    @Test fun invalidPurposeAndActionCannotReachTheSystemPicker() {
        assertEquals("unsupported-purpose", validate(purpose = "protected-configuration-export"))
        assertEquals("purpose-action-mismatch", validate(purpose = "settings-import"))
        assertEquals("purpose-action-mismatch", validate(action = "pickRead"))
        assertEquals("purpose-action-mismatch", validate(action = "unknown"))
    }
    @Test fun pluginPackagesAreBoundedReadOnlySystemSelections() {
        assertNull(validate(action = "pickRead", purpose = "plugin-package-import", budget = 16 * 1024 * 1024L, name = null, body = null))
        assertEquals("invalid-byte-budget", validate(action = "pickRead", purpose = "plugin-package-import", budget = 16 * 1024 * 1024L + 1, name = null, body = null))
        assertEquals("purpose-action-mismatch", validate(purpose = "plugin-package-import"))
    }

    @Test fun visiblePluginDataUsesItsOwnFourMiBBudgetAndReadOnlyPicker() {
        assertNull(validate(action="pickRead", purpose="plugin-visible-file-import", budget=4*1024*1024L, name=null, body=null))
        assertEquals("invalid-byte-budget", validate(action="pickRead", purpose="plugin-visible-file-import", budget=4*1024*1024L+1, name=null, body=null))
        assertEquals("purpose-action-mismatch", validate(purpose="plugin-visible-file-import"))
        assertEquals("invalid-import-arguments", validate(action="pickRead", purpose="plugin-visible-file-import", name=null))
    }
    @Test fun visiblePluginExportCannotMasqueradeAsProtectedOrAnAcknowledgedShare() {
        assertEquals("credentials-acknowledgement-required", validate(purpose="plugin-visible-file-export", mime="application/octet-stream"))
        assertNull(validate(purpose="plugin-visible-file-export", mime="application/octet-stream", acknowledged=true))
        assertEquals("invalid-byte-budget", validate(purpose="plugin-visible-file-export", budget=4*1024*1024L+1, acknowledged=true))
        assertEquals("purpose-action-mismatch", validate(action="share", purpose="plugin-visible-file-export", acknowledged=true))
        assertEquals("unsupported-purpose", validate(purpose="protected-plugin-file-export", acknowledged=true))
        assertEquals("unsupported-mime-type", validate(purpose="settings-export", mime="application/octet-stream"))
    }

    @Test fun budgetsUsePurposeSpecificLimits() {
        assertEquals("invalid-byte-budget", validate(budget = 0))
        assertEquals("invalid-byte-budget", validate(budget = -1))
        assertEquals("invalid-byte-budget", validate(budget = 2 * 1024 * 1024L + 1))
        assertNull(validate(purpose = "ordinary-configuration-export", budget = 16 * 1024 * 1024L, acknowledged = true))
        assertEquals("invalid-byte-budget", validate(purpose = "redacted-diagnostic-export", budget = 10 * 1024 * 1024L + 1))
    }

    @Test fun ordinaryExportNeedsSeparateAcknowledgement() {
        assertEquals("credentials-acknowledgement-required", validate(purpose = "ordinary-configuration-export"))
        assertNull(validate(purpose = "ordinary-configuration-export", acknowledged = true))
    }

    @Test fun namesNeverEscapePrivateStagingDirectory() {
        for (name in listOf("../settings.json", "/tmp/settings.json", "a\\b", ".", "..", "a:b", "a\n", "a".repeat(129))) {
            assertEquals("invalid-file-name", validate(name = name))
        }
        assertNull(validate(name = "diagnostic-1.json"))
        assertEquals("invalid-file-name", validate(name = null))
    }

    @Test fun importAcceptsOnlyImportArguments() {
        assertNull(validate(action = "pickRead", purpose = "configuration-import", name = null, body = null))
        assertEquals("invalid-import-arguments", validate(action = "pickRead", purpose = "settings-import", name = null))
        assertEquals("invalid-import-arguments", validate(action = "pickRead", purpose = "settings-import", name = null, body = null, acknowledged = true))
    }

    @Test fun encodedAndDecodedBudgetsAreCheckedBeforeDecode() {
        assertEquals("byte-budget-exceeded", validate(budget = 1, body = "YWJjZA=="))
        assertEquals("byte-budget-exceeded", validate(budget = 1, body = "YWI="))
        assertNull(validate(budget = 1, body = "YQ=="))
        for (body in listOf("a", "!!!!", "YQ=", "YQ==\n", "=AAA", "====")) {
            assertEquals("invalid-base64", validate(body = body))
        }
    }

    @Test fun formatsDoNotBroadenConfigurationContracts() {
        assertEquals("unsupported-mime-type", validate(mime = "text/plain"))
        assertEquals("unsupported-mime-type", validate(mime = "application/zip"))
        assertEquals("unsupported-mime-type", validate(purpose = "redacted-diagnostic-export", mime = "*/*"))
        assertNull(validate(purpose = "redacted-diagnostic-export", mime = "application/zip"))
    }

    @Test fun boundedReadAllowsExactSizeAndRejectsOversizeWithoutPartialReturn() {
        assertArrayEquals(byteArrayOf(1, 2, 3), NativeIoPolicy.readBounded(ByteArrayInputStream(byteArrayOf(1, 2, 3)), 3))
        assertArrayEquals(byteArrayOf(), NativeIoPolicy.readBounded(ByteArrayInputStream(byteArrayOf()), 3))
        try {
            NativeIoPolicy.readBounded(ByteArrayInputStream(byteArrayOf(1, 2, 3, 4)), 3)
            fail("oversize content must never succeed")
        } catch (_: ByteBudgetExceeded) { }
    }

    @Test fun zeroProgressProviderCannotCauseAnInfiniteReadLoop() {
        val stream = object : InputStream() {
            var remaining = 3
            override fun read(buffer: ByteArray, offset: Int, length: Int) = 0
            override fun read() = if (remaining-- > 0) 42 else -1
        }
        assertArrayEquals(byteArrayOf(42, 42, 42), NativeIoPolicy.readBounded(stream, 3))
    }

    @Test fun expiredOrUntrustworthyShareTimestampNeverReopensContent() {
        val created = 12345L
        assertFalse(NativeIoPolicy.shareExpired(created, created + NativeIoPolicy.SHARE_TTL_MS - 1))
        assertTrue(NativeIoPolicy.shareExpired(created, created + NativeIoPolicy.SHARE_TTL_MS))
        assertTrue(NativeIoPolicy.shareExpired(created, created - 1))
        assertTrue(NativeIoPolicy.shareExpired(0, created))
    }
}
