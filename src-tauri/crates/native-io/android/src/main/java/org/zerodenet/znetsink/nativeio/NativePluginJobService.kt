package org.zerodenet.znetsink.nativeio

import android.app.job.JobParameters
import android.app.job.JobService
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.annotation.Keep
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicReference

/** OS-owned bounded plugin execution, including cold starts without Tauri or an Activity. */
@Keep
class NativePluginJobService : JobService() {
    private class Run(val parameters: JobParameters) {
        val gate = NativePluginRunGate(UUID.randomUUID().toString().replace("-", ""))
        lateinit var timeout: Runnable
    }
    private val active = AtomicReference<Run?>(null)
    private val worker = Executors.newSingleThreadExecutor()
    private val cancellation = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    override fun onStartJob(parameters: JobParameters): Boolean {
        if (parameters.jobId != NativePluginJobs.JOB_ID) return false
        val run = Run(parameters)
        if (!active.compareAndSet(null, run)) return false // An earlier native call is draining.
        if (!NativePluginJobs.started(applicationContext, run.gate.id)) {
            active.compareAndSet(run, null)
            return false // Durable observation is required before dispatch.
        }
        run.timeout = Runnable {
            if (active.get() === run && run.gate.stop()) {
                journal { NativePluginJobs.stopped(applicationContext, run.gate.id, "android-job-time-budget-exhausted") }
                cancelNative(run)
                if (run.gate.claimCallback()) jobFinished(run.parameters, false)
            }
        }
        main.postDelayed(run.timeout, NativePluginJobs.RUN_BUDGET_MS)
        try { worker.execute { execute(run) } }
        catch (_: RejectedExecutionException) {
            main.removeCallbacks(run.timeout)
            run.gate.complete()
            journal { NativePluginJobs.failed(applicationContext, run.gate.id, "android-worker-unavailable") }
            active.compareAndSet(run, null)
            return false
        }
        return true
    }

    private fun execute(run: Run) {
        try {
            NativePluginJobs.loadNative()
            if (run.gate.stopped()) return
            val receipt = NativePluginJobs.nativeRun(applicationContext, run.gate.id)
            if (run.gate.complete()) {
                try { NativePluginJobs.completed(applicationContext, run.gate.id, receipt) }
                catch (_: Exception) { journal { NativePluginJobs.failed(applicationContext, run.gate.id, "android-native-receipt-unconfirmed") } }
            }
        } catch (_: Exception) {
            if (run.gate.complete()) journal { NativePluginJobs.failed(applicationContext, run.gate.id, "android-native-execution-unconfirmed") }
        } catch (_: LinkageError) {
            if (run.gate.complete()) journal { NativePluginJobs.failed(applicationContext, run.gate.id, "android-native-runtime-unavailable") }
        } finally {
            main.post {
                main.removeCallbacks(run.timeout)
                if (active.compareAndSet(run, null) && run.gate.claimCallback()) jobFinished(run.parameters, false)
            }
        }
    }

    override fun onStopJob(parameters: JobParameters): Boolean {
        val run = active.get() ?: return false
        // Only JobScheduler can bind this service. Platform lifecycle callbacks
        // are serialized on main; JobParameters reference equality is not an API
        // contract across Binder calls. Late worker deliveries use the run gate.
        if (run.parameters.jobId != parameters.jobId) return false
        if (run.gate.stop()) {
            val reason = if (Build.VERSION.SDK_INT >= 31) parameters.stopReason else null
            journal { NativePluginJobs.stopped(applicationContext, run.gate.id, "android-system-stopped-job", reason) }
            cancelNative(run)
        }
        main.removeCallbacks(run.timeout)
        run.gate.claimCallback() // Android owns completion after onStopJob; never call jobFinished.
        // The periodic intent remains registered. Unknown dispatched attempts are not replayed.
        return false
    }

    override fun onDestroy() {
        active.get()?.let { run ->
            if (run.gate.stop()) {
                journal { NativePluginJobs.stopped(applicationContext, run.gate.id, "android-job-service-destroyed") }
                cancelNative(run)
            }
            main.removeCallbacks(run.timeout)
            run.gate.claimCallback()
        }
        worker.shutdown()
        cancellation.shutdown()
        super.onDestroy()
    }

    private fun cancelNative(run: Run) {
        // nativeStop only seals a bounded in-memory run/cancellation token. If
        // loaded, seal Rust admission before returning to Android, so its queue
        // cannot dispatch another target while a cancellation worker is pending.
        // Never load the shared library or perform storage IO on this path.
        if (NativePluginJobs.nativeReady()) {
            try { NativePluginJobs.nativeStop(run.gate.id); return }
            catch (_: Exception) { /* Fall back to a bounded worker attempt. */ }
            catch (_: LinkageError) { /* The Java delivery gate remains sealed. */ }
        }
        try { cancellation.execute {
            try { NativePluginJobs.loadNative(); NativePluginJobs.nativeStop(run.gate.id) }
            catch (_: Exception) { /* Delivery is already sealed; Rust journal recovers uncertainty. */ }
            catch (_: LinkageError) { /* No native execution can be assumed to have completed. */ }
        } } catch (_: RejectedExecutionException) { /* Stop remains durable and delivery sealed. */ }
    }

    private inline fun journal(operation: () -> Unit) {
        try { operation() } catch (_: Exception) { /* Keep original running checkpoint for recovery. */ }
    }
}
