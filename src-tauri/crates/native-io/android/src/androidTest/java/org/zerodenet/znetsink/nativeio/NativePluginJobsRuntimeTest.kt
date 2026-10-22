package org.zerodenet.znetsink.nativeio

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.content.pm.ApplicationInfo
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Real Android scheduler registration and private durable journal; no UI/VM execution claim. */
@RunWith(AndroidJUnit4::class)
class NativePluginJobsRuntimeTest {
    @Test fun persistedSystemJobAndRunJournalPreserveUnknownAndRejectLateResults() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        check(context.packageName.startsWith("org.zerodenet.znetsink.nativeio") && context.packageName.endsWith(".test"))
        check(context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0)
        check(Build.VERSION.SDK_INT >= 24)
        val scheduler = context.getSystemService(Context.JOB_SCHEDULER_SERVICE) as JobScheduler
        val directory = File(context.noBackupFilesDir, "znet-plugin-jobs-v1")
        // Fail on an unowned/reused fixture instead of deleting its state or pending job.
        check(!directory.exists() && scheduler.getPendingJob(NativePluginJobs.JOB_ID) == null)
        try {
            val scheduled = JSONObject(NativePluginJobs.ensure(context))
            assertEquals("scheduled", scheduled.getJSONObject("scheduling").getString("state"))
            val job = scheduler.getPendingJob(NativePluginJobs.JOB_ID)!!
            assertTrue(job.isPersisted)
            assertTrue(job.isPeriodic)
            assertEquals(NativePluginJobs.PERIOD_MS, job.intervalMillis)
            assertEquals(JobInfo.NETWORK_TYPE_ANY, job.networkType)
            assertEquals(ComponentName(context, NativePluginJobService::class.java), job.service)
            val service = context.packageManager.getServiceInfo(job.service, 0)
            assertEquals("android.permission.BIND_JOB_SERVICE", service.permission)
            assertEquals(context.applicationInfo.processName, service.processName)
            val repeated = JSONObject(NativePluginJobs.ensure(context))
            assertEquals("scheduled", repeated.getJSONObject("scheduling").getString("state"))

            val first = "11111111111111111111111111111111"
            assertTrue(NativePluginJobs.started(context, first))
            val completed = """{"state":"completed","completed":2,"failed":0,"unconfirmed":0}"""
            NativePluginJobs.completed(context, first, completed)
            var run = JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun")
            assertEquals("completed", run.getString("state"))
            assertEquals(2, run.getInt("completed"))
            NativePluginJobs.stopped(context, first, "android-system-stopped-job")
            assertEquals("completed", JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun").getString("state"))

            val second = "22222222222222222222222222222222"
            assertTrue(NativePluginJobs.started(context, second))
            NativePluginJobs.stopped(context, second, "android-system-stopped-job")
            NativePluginJobs.completed(context, second, completed)
            run = JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun")
            assertEquals("stopped", run.getString("state"))
            assertFalse(run.getBoolean("resultConfirmed"))
            assertFalse(run.has("completed")) // No invented zero or successful late receipt.

            val third = "33333333333333333333333333333333"
            assertTrue(NativePluginJobs.started(context, third))
            for (invalid in listOf(
                """{"state":"completed","completed":9,"failed":0,"unconfirmed":0}""",
                """{"state":"completed","completed":8,"failed":1,"unconfirmed":0}""",
                """{"state":"completed","completed":0,"failed":0,"unconfirmed":0,"secret":"must-not-persist"}""",
                """{"state":"failed","completed":0,"failed":1,"unconfirmed":0,"reason":"url?token=secret"}"""
            )) {
                try { NativePluginJobs.completed(context, third, invalid); fail("invalid receipt admitted") }
                catch (_: IllegalArgumentException) { }
            }
            assertEquals("running", JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun").getString("state"))

            // Model a previous process's durable checkpoint using only this test's private fixture.
            val file = File(directory, "state.json")
            val previous = JSONObject(file.readText())
            previous.getJSONObject("lastRun").put("processToken", "previous-test-process")
            file.writeText(previous.toString())
            run = JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun")
            assertEquals("result-unknown", run.getString("state"))
            assertEquals("android-process-recreated", run.getString("reason"))
            assertFalse(run.getBoolean("resultConfirmed"))
            NativePluginJobs.completed(context, third, completed)
            assertEquals("result-unknown", JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun").getString("state"))

            val fourth = "44444444444444444444444444444444"
            assertTrue(NativePluginJobs.started(context, fourth))
            NativePluginJobs.completed(context, fourth, """{"state":"failed","reason":"native-run-unconfirmed"}""")
            run = JSONObject(NativePluginJobs.snapshot(context)).getJSONObject("lastRun")
            assertEquals("failed", run.getString("state"))
            assertFalse(run.getBoolean("resultConfirmed"))
            assertFalse(run.has("completed"))
            assertFalse(run.has("failed"))
            assertFalse(run.has("unconfirmed"))
        } finally {
            scheduler.cancel(NativePluginJobs.JOB_ID) // Only the exact test-owned periodic job.
            check(directory.deleteRecursively())
        }
    }
}
