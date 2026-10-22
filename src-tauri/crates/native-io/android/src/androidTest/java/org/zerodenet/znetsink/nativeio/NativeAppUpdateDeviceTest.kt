package org.zerodenet.znetsink.nativeio

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.Assume.assumeFalse
import org.junit.runner.RunWith
import java.io.File
import java.security.MessageDigest

/** Real PackageManager / APK parsing tests; no system permission changes, installs or credentials. */
@RunWith(AndroidJUnit4::class)
class NativeAppUpdateDeviceTest {
    @Test fun installedArchiveCannotBeInstalledAgainAsANewerUpdate() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(context.applicationInfo.sourceDir)
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { stream -> val buffer = ByteArray(64 * 1024); while (true) { val n = stream.read(buffer); if (n < 0) break; digest.update(buffer, 0, n) } }
        @Suppress("DEPRECATION") val installed = context.packageManager.getPackageInfo(context.packageName, 0)
        val request = JSONObject().put("size", file.length()).put("sha256", NativeAppUpdates.hex(digest.digest())).put("version", installed.versionName)
        try { NativeAppUpdates.verify(context, request, file); fail("same installed APK must not pass newer-version check") }
        catch (error: IllegalStateException) { assertEquals("version-not-newer", error.message) }
    }
    @Test fun digestMismatchIsRejectedBeforePackageParsing() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(context.cacheDir, "update-device-test.invalid")
        try {
            file.writeBytes(byteArrayOf(1, 2, 3))
            val request = JSONObject().put("size", 3).put("sha256", "0".repeat(64)).put("version", "1.0.0")
            try { NativeAppUpdates.verify(context, request, file); fail("bad digest must fail") }
            catch (error: IllegalStateException) { assertEquals("digest-mismatch", error.message) }
        } finally { file.delete() }
    }
    @Test fun forgedCallbackDoesNotCreateAnOperation() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val before = synchronized(NativeAppUpdates.lock) { NativeAppUpdates.load(context)?.toString() }
        NativeAppUpdates.installerResult(context, android.content.Intent("forged-update-result").setData(android.net.Uri.parse("znet-app-update://forged/1")))
        val after = synchronized(NativeAppUpdates.lock) { NativeAppUpdates.load(context)?.toString() }
        assertEquals(before, after)
    }
    @Test fun atomicBackupRecoveryUsesTheActualReadBudget() {
        // Separate device-protected test context; never overwrite the credential-protected owner journal.
        val context = InstrumentationRegistry.getInstrumentation().targetContext.createDeviceProtectedStorageContext()
        synchronized(NativeAppUpdates.lock) {
            val directory = File(context.noBackupFilesDir, "app-update")
            val journal = File(directory, "operation.json")
            val backup = File(directory, "operation.json.bak")
            assumeFalse(journal.exists() || backup.exists())
            check(directory.exists() || directory.mkdirs())
            try {
                backup.writeBytes(ByteArray(64 * 1024 + 1) { 'x'.code.toByte() })
                try { NativeAppUpdates.load(context); fail("restored backup must be byte bounded") }
                catch (_: ByteBudgetExceeded) { /* Expected before JSON parsing. */ }
            } finally { journal.delete(); backup.delete() }
        }
    }
    @Test fun interruptedPrepareAndMissingCommitAcknowledgementNeverReplayASession() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext.createDeviceProtectedStorageContext()
        synchronized(NativeAppUpdates.lock) {
            val directory = File(context.noBackupFilesDir, "app-update")
            val journal = File(directory, "operation.json")
            val backup = File(directory, "operation.json.bak")
            assumeFalse(journal.exists() || backup.exists())
            val installer = context.packageManager.packageInstaller
            // Create one empty test-owned session; never write or commit an APK and always abandon it.
            val parameters = android.content.pm.PackageInstaller.SessionParams(android.content.pm.PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            parameters.setAppPackageName(context.packageName)
            val sessionId = installer.createSession(parameters)
            val sessions = installer.mySessions.map { it.sessionId }.toSet()
            try {
                for (stage in listOf("preparing-install", "installing")) {
                    val record = JSONObject().put("schema", 1).put("operationId", "e2695293-5580-4eec-9437-a1e6b2621111")
                        .put("sha256", "a".repeat(64)).put("assetName", "client.apk").put("version", "1.1.0")
                        .put("assetUrl", "https://github.com/zerodenet/znet-sink/releases/download/v1.1.0/client.apk")
                        .put("size", 100).put("downloaded", 100).put("createdAt", System.currentTimeMillis())
                        .put("attempts", 1).put("seen", org.json.JSONArray()).put("state", stage)
                        .put("sessionId", sessionId).put("callbackToken", "f2695293-5580-4eec-9437-a1e6b2621111")
                    NativeAppUpdates.save(context, record)
                    NativeAppUpdates.reconcile(context)
                    assertEquals("unknown", NativeAppUpdates.load(context)!!.getString("state"))
                    assertEquals(sessions, context.packageManager.packageInstaller.mySessions.map { it.sessionId }.toSet())
                }
            } finally { try { installer.abandonSession(sessionId) } finally { journal.delete(); backup.delete() } }
        }
    }
    @Test fun missingInstallerSessionInvalidatesAnOtherwiseLiveConfirmationToken() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext.createDeviceProtectedStorageContext()
        synchronized(NativeAppUpdates.lock) {
            val directory = File(context.noBackupFilesDir, "app-update")
            val journal = File(directory, "operation.json")
            val backup = File(directory, "operation.json.bak")
            assumeFalse(journal.exists() || backup.exists())
            val installer = context.packageManager.packageInstaller
            val initialSessions = installer.mySessions.map { it.sessionId }.toSet()
            val params = android.content.pm.PackageInstaller.SessionParams(android.content.pm.PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            params.setAppPackageName(context.packageName)
            val sessionId = installer.createSession(params)
            // Real app-owned unsent token: its existence alone proves no installer/session state.
            val intent = android.content.Intent(android.content.Intent.ACTION_MAIN).setPackage(context.packageName)
                .setData(android.net.Uri.parse("znet-test-install://missing-session/$sessionId"))
            val token = android.app.PendingIntent.getActivity(context, sessionId, intent,
                android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE)
            val operationId = "e2695293-5580-4eec-9437-a1e6b2622222"
            try {
                val record = JSONObject().put("schema", 1).put("operationId", operationId)
                    .put("sha256", "a".repeat(64)).put("assetName", "client.apk").put("version", "1.1.0")
                    .put("assetUrl", "https://github.com/zerodenet/znet-sink/releases/download/v1.1.0/client.apk")
                    .put("size", 100).put("downloaded", 100).put("createdAt", System.currentTimeMillis())
                    .put("attempts", 1).put("seen", org.json.JSONArray()).put("state", "awaiting-user")
                    .put("sessionId", sessionId).put("callbackToken", "f2695293-5580-4eec-9437-a1e6b2622222")
                    .put("confirmationIdentity", intent.cloneFilter().toUri(0))
                NativeAppUpdates.save(context, record)
                installer.abandonSession(sessionId) // No APK written/committed; intentionally no final callback.
                val deadline = System.nanoTime() + 2_000_000_000L
                while (installer.getSessionInfo(sessionId) != null && System.nanoTime() < deadline) Thread.sleep(20)
                assertNull(installer.getSessionInfo(sessionId))
                assertNotNull(android.app.PendingIntent.getActivity(context, sessionId, intent,
                    android.app.PendingIntent.FLAG_NO_CREATE or android.app.PendingIntent.FLAG_IMMUTABLE))
                val result = NativeAppUpdates.snapshot(context)
                assertEquals("unknown", result.getString("state"))
                assertEquals(operationId, result.getString("operationId"))
                assertFalse(result.getBoolean("confirmationAvailable"))
                assertEquals(initialSessions, installer.mySessions.map { it.sessionId }.toSet())
            } finally {
                token.cancel()
                try { installer.abandonSession(sessionId) } catch (_: Exception) { }
                journal.delete(); backup.delete()
            }
        }
    }

}
