package org.zerodenet.znetsink.nativeio

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.util.AtomicFile
import androidx.annotation.Keep
import org.json.JSONObject
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean

/** A single OS wake-up coalesces the existing Rust task queue; it owns no plugin permission. */
@Keep
object NativePluginJobs {
    const val JOB_ID = 0x5a4e5301
    const val PERIOD_MS = 15 * 60 * 1000L
    const val RUN_BUDGET_MS = 90 * 1000L
    private const val MAX_JOURNAL_BYTES = 16 * 1024L
    private val processToken = UUID.randomUUID().toString()
    private val nativeLoaded = AtomicBoolean(false)

    @JvmStatic external fun nativeRun(context: Context, runId: String): String
    @JvmStatic external fun nativeStop(runId: String)

    /** No background key creation, permission prompt, Activity or ordinary-file key fallback. */
    @JvmStatic fun readVault(context: Context): ByteArray? = NativeVault(context.applicationContext).load(false)

    internal fun loadNative() {
        if (nativeLoaded.get()) return
        synchronized(nativeLoaded) {
            if (!nativeLoaded.get()) {
                System.loadLibrary("gui_lib")
                nativeLoaded.set(true)
            }
        }
    }

    internal fun nativeReady(): Boolean = nativeLoaded.get()

    internal fun jobInfo(context: Context): JobInfo = JobInfo.Builder(
        JOB_ID, ComponentName(context, NativePluginJobService::class.java)
    ).setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
        .setPersisted(true)
        .setPeriodic(PERIOD_MS)
        .build()

    /** Rechecking an existing job must not keep pushing its periodic window into the future. */
    @Synchronized fun ensure(context: Context): String {
        val app = context.applicationContext
        return try {
            val journal = read(app)
            recover(journal)
            val scheduler = app.getSystemService(Context.JOB_SCHEDULER_SERVICE) as? JobScheduler
            val current = scheduler?.getPendingJob(JOB_ID)
            val expected = jobInfo(app)
            val conflict = current != null && current.service != expected.service
            val matches = current != null && current.service == expected.service && current.isPeriodic &&
                current.isPersisted && current.intervalMillis == PERIOD_MS && current.networkType == JobInfo.NETWORK_TYPE_ANY
            val scheduled = scheduler != null && !conflict && (matches || scheduler.schedule(expected) == JobScheduler.RESULT_SUCCESS)
            journal.put("scheduling", JSONObject()
                .put("state", if (scheduled) "scheduled" else "unavailable")
                .put("jobId", JOB_ID)
                .put("periodMs", PERIOD_MS)
                .put("networkRequired", true)
                .put("bestEffort", true)
                .put("observedAtUnixMs", System.currentTimeMillis().toString())
                .put("reason", if (scheduled) "android-system-deferred-periodic-job" else if (conflict)
                    "android-job-definition-conflict" else "android-job-scheduling-unavailable"))
            write(app, journal)
            journal.toString()
        } catch (_: Exception) {
            unavailable("android-job-state-unavailable").toString()
        }
    }

    @Synchronized fun snapshot(context: Context): String = try {
        val journal = read(context.applicationContext)
        if (recover(journal)) write(context.applicationContext, journal)
        journal.toString()
    } catch (_: Exception) { unavailable("android-job-state-unavailable").toString() }

    /** A durable start checkpoint is required before crossing into Rust or guest execution. */
    @Synchronized internal fun started(context: Context, id: String): Boolean = try {
        require(NativePluginRunGate.validId(id))
        val journal = read(context)
        recover(journal)
        journal.put("lastRun", JSONObject()
            .put("runId", id).put("processToken", processToken)
            .put("origin", "android-job-scheduler").put("state", "running")
            .put("startedAtUnixMs", System.currentTimeMillis().toString()))
        write(context, journal)
        true
    } catch (_: Exception) { false }

    @Synchronized internal fun completed(context: Context, id: String, raw: String) {
        val receipt = parseReceipt(raw)
        finish(context, id, receipt)
    }

    @Synchronized internal fun stopped(context: Context, id: String, reason: String, stopReason: Int? = null) {
        val receipt = JSONObject().put("state", "stopped").put("reason", reason)
            .put("resultConfirmed", false)
        if (stopReason != null) receipt.put("androidStopReason", stopReason)
        finish(context, id, receipt)
    }

    @Synchronized internal fun failed(context: Context, id: String, reason: String) {
        finish(context, id, JSONObject().put("state", "result-unknown")
            .put("reason", reason).put("resultConfirmed", false))
    }

    private fun parseReceipt(raw: String): JSONObject {
        require(raw.toByteArray(Charsets.UTF_8).size <= 2048)
        val value = JSONObject(raw)
        val allowed = setOf("state", "completed", "failed", "unconfirmed", "reason")
        require(value.keys().asSequence().all { it in allowed })
        val state = value.getString("state")
        require(state in setOf("completed", "deferred", "failed", "stopped"))
        val countNames = listOf("completed", "failed", "unconfirmed")
        val unknownCounts = countNames.none { value.has(it) }
        if (unknownCounts) {
            // A native bootstrap/panic failure has no authoritative task totals.
            require(state == "failed" && value.optString("reason") == "native-run-unconfirmed")
        } else {
            for (name in countNames) {
                val count = value.get(name)
                require(count is Int && count in 0..8)
            }
            require(value.getInt("completed") + value.getInt("failed") + value.getInt("unconfirmed") <= 8)
        }
        if (value.has("reason") && !value.isNull("reason")) {
            require(value.get("reason") is String)
            require(value.getString("reason").matches(Regex("[a-z0-9_-]{1,120}")))
        }
        value.put("resultConfirmed", !unknownCounts && value.getInt("unconfirmed") == 0 && state != "stopped")
        return value
    }

    private fun finish(context: Context, id: String, receipt: JSONObject) {
        val journal = read(context)
        val run = journal.optJSONObject("lastRun") ?: return
        // A stopped, replaced or process-recovered run cannot accept a late receipt.
        if (run.optString("runId") != id || run.optString("processToken") != processToken || run.optString("state") != "running") return
        receipt.keys().asSequence().forEach { key -> run.put(key, receipt.get(key)) }
        run.put("endedAtUnixMs", System.currentTimeMillis().toString())
        write(context, journal)
    }

    private fun recover(journal: JSONObject): Boolean {
        val run = journal.optJSONObject("lastRun") ?: return false
        if (!NativePluginRunGate.needsRecovery(run.optString("state"), run.optString("processToken"), processToken)) return false
        run.put("state", "result-unknown").put("reason", "android-process-recreated")
            .put("resultConfirmed", false).put("observedAtUnixMs", System.currentTimeMillis().toString())
        return true
    }

    private fun journalFile(context: Context): AtomicFile {
        val directory = File(context.noBackupFilesDir, "znet-plugin-jobs-v1")
        check(directory.isDirectory || directory.mkdirs())
        return AtomicFile(File(directory, "state.json"))
    }

    private fun read(context: Context): JSONObject {
        val file = journalFile(context)
        if (!file.baseFile.exists() && !File(file.baseFile.path + ".bak").exists()) return JSONObject().put("schemaVersion", 1)
        val bytes = file.openRead().use { NativeIoPolicy.readBounded(it, MAX_JOURNAL_BYTES) }
        val value = JSONObject(String(bytes, Charsets.UTF_8))
        require(value.optInt("schemaVersion") == 1)
        return value
    }

    private fun write(context: Context, value: JSONObject) {
        val bytes = value.toString().toByteArray(Charsets.UTF_8)
        require(bytes.size <= MAX_JOURNAL_BYTES)
        val file = journalFile(context)
        val output = file.startWrite()
        try { output.write(bytes); file.finishWrite(output) }
        catch (failure: Exception) { file.failWrite(output); throw failure }
    }

    private fun unavailable(reason: String): JSONObject = JSONObject().put("schemaVersion", 1)
        .put("scheduling", JSONObject().put("state", "unavailable").put("reason", reason))
}

/** JVM-testable ownership gate; stopping seals delivery before requesting native cancellation. */
internal class NativePluginRunGate(val id: String) {
    private enum class Phase { RUNNING, COMPLETED, STOPPED }
    private var phase = Phase.RUNNING
    private var callbackClaimed = false
    init { require(validId(id)) }
    @Synchronized fun complete(): Boolean {
        if (phase != Phase.RUNNING) return false
        phase = Phase.COMPLETED
        return true
    }
    @Synchronized fun stop(): Boolean {
        if (phase != Phase.RUNNING) return false
        phase = Phase.STOPPED
        return true
    }
    @Synchronized fun stopped(): Boolean = phase == Phase.STOPPED
    @Synchronized fun claimCallback(): Boolean {
        if (callbackClaimed) return false
        callbackClaimed = true
        return true
    }
    companion object {
        fun validId(value: String): Boolean = value.matches(Regex("[0-9a-f]{32}"))
        fun needsRecovery(state: String, storedProcess: String, currentProcess: String): Boolean =
            state == "running" && storedProcess != currentProcess
    }
}
