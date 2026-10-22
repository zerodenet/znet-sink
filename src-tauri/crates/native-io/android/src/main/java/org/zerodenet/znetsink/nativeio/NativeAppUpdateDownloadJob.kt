package org.zerodenet.znetsink.nativeio

import android.app.job.JobParameters
import android.app.job.JobService
import org.json.JSONObject
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** OS-owned, restartable bounded download. A stopped attempt restarts from byte zero, never appends untrusted partial data. */
class NativeAppUpdateDownloadJob : JobService() {
    private val worker = Executors.newSingleThreadExecutor()
    @Volatile private var activeStop = AtomicBoolean(false)
    @Volatile private var connection: HttpURLConnection? = null
    override fun onStartJob(params: JobParameters): Boolean {
        val stopped = AtomicBoolean(false)
        activeStop = stopped
        worker.execute {
            val retry = runDownload(stopped)
            if (!stopped.get()) jobFinished(params, retry)
        }
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean {
        activeStop.set(true); connection?.disconnect()
        // Android may defer retry under Doze, network, battery, quota or force-stop restrictions.
        return true
    }
    override fun onDestroy() { activeStop.set(true); connection?.disconnect(); worker.shutdown(); super.onDestroy() }

    private fun runDownload(stopped: AtomicBoolean): Boolean {
        var id: String? = null
        try {
            val current = synchronized(NativeAppUpdates.lock) {
                val c = NativeAppUpdates.load(this) ?: return false
                if (c.getString("state") !in setOf("queued", "paused", "downloading", "verifying")) return false
                id = c.getString("operationId")
                check(System.currentTimeMillis() - c.getLong("createdAt") in 0..NativeAppUpdatePolicy.MAX_AGE_MS) { "download-intent-expired" }
                val attempts = c.getInt("attempts") + 1
                check(attempts <= 8) { "download-retry-budget" }
                check(NativeAppUpdates.hasSpace(this, c.getLong("size"))) { "storage-budget-unavailable" }
                c.put("attempts", attempts).put("state", "downloading").put("downloaded", 0).put("reason", JSONObject.NULL)
                NativeAppUpdates.save(this, c)
                JSONObject(c.toString())
            }
            val size = current.getLong("size")
            val file = NativeAppUpdates.partial(this, id!!)
            var url = current.getString("assetUrl")
            var redirects = 0
            var response: HttpURLConnection
            while (true) {
                checkActive(id!!, stopped)
                check(NativeAppUpdatePolicy.redirect(url)) { "download-origin-rejected" }
                response = URL(url).openConnection() as HttpURLConnection
                connection = response
                response.instanceFollowRedirects = false; response.connectTimeout = 15_000; response.readTimeout = 30_000
                response.useCaches = false; response.setRequestProperty("User-Agent", "ZNet-Sink-Android-update")
                response.setRequestProperty("Accept-Encoding", "identity")
                val status = response.responseCode
                if (status in setOf(301, 302, 303, 307, 308)) {
                    check(++redirects <= 4) { "download-redirect-budget" }
                    val location = response.getHeaderField("Location") ?: error("download-redirect-invalid")
                    url = URL(URL(url), location).toString(); response.disconnect(); continue
                }
                check(status == 200) { "download-http-rejected" }
                val length = response.contentLengthLong
                check(length == -1L || length == size) { "download-size-mismatch" }
                break
            }
            val started = System.nanoTime()
            var savedAt = 0L
            response.inputStream.use { input -> FileOutputStream(file, false).use { output ->
                NativeAppUpdatePolicy.transfer(input, output, size, { checkActive(id!!, stopped) }, { received ->
                    if (received - savedAt >= 1024 * 1024 || received == size) {
                        synchronized(NativeAppUpdates.lock) {
                            val latest = current(id!!)
                            latest.put("downloaded", received); NativeAppUpdates.save(this, latest)
                        }
                        savedAt = received
                    }
                }, { System.nanoTime() - started })
                output.fd.sync()
            } }
            synchronized(NativeAppUpdates.lock) {
                val latest = current(id!!); latest.put("state", "verifying"); NativeAppUpdates.save(this, latest)
                NativeAppUpdates.verify(this, latest, file)
                check(!stopped.get()) { "os-job-stopped" }
                check(file.renameTo(NativeAppUpdates.apk(this, id!!))) { "staging-commit-failed" }
                latest.put("state", "ready").put("reason", "apk-checked-awaiting-install-review")
                NativeAppUpdates.save(this, latest)
            }
            return false
        } catch (error: Exception) {
            synchronized(NativeAppUpdates.lock) {
                val latest = NativeAppUpdates.load(this)
                if (latest != null && latest.optString("operationId") == id && latest.getString("state") in setOf("queued", "downloading", "paused", "verifying")) {
                    // Never return exception text from remote HTTP, certificate or APK parsers.
                    val known = error.message?.takeIf { it in setOf(
                        "download-intent-expired", "download-retry-budget", "storage-budget-unavailable",
                        "download-origin-rejected", "download-redirect-budget", "download-redirect-invalid",
                        "download-http-rejected", "download-size-mismatch", "download-attempt-timeout",
                        "download-size-budget", "download-truncated", "os-job-stopped", "staging-commit-failed",
                        "size-mismatch", "digest-mismatch", "apk-entry-budget", "apk-expanded-budget",
                        "invalid-native-library", "invalid-apk", "package-mismatch", "version-not-newer",
                        "version-name-mismatch", "sdk-incompatible", "signer-mismatch", "abi-incompatible"
                    ) }
                    val verifying = latest.getString("state") == "verifying"
                    val permanent = verifying && known != "os-job-stopped" || known != null && known !in setOf("download-attempt-timeout", "download-truncated", "os-job-stopped")
                    val retry = !permanent && latest.optInt("attempts") < 8 && System.currentTimeMillis() - latest.getLong("createdAt") in 0..NativeAppUpdatePolicy.MAX_AGE_MS
                    latest.put("state", if (retry) "paused" else "failed").put("reason", known ?: if (verifying) "apk-verification-failed" else "download-network-interrupted")
                    NativeAppUpdates.save(this, latest)
                    id?.let { NativeAppUpdates.partial(this, it).delete() }
                    return retry
                }
            }
            return false
        } finally { connection?.disconnect(); connection = null }
    }
    private fun checkActive(id: String, stopped: AtomicBoolean) { check(!stopped.get()) { "os-job-stopped" }; synchronized(NativeAppUpdates.lock) { current(id) } }
    private fun current(id: String): JSONObject {
        val value = NativeAppUpdates.load(this) ?: error("operation-missing")
        check(value.getString("operationId") == id && value.getString("state") in setOf("queued", "downloading", "paused", "verifying")) { "operation-no-longer-active" }
        return value
    }
}
