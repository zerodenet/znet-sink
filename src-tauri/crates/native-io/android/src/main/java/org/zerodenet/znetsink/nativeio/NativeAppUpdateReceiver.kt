package org.zerodenet.znetsink.nativeio

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import java.util.concurrent.Executors

/** Explicit private PendingIntent target. Callback token + exact session ID are rechecked by owner. */
class NativeAppUpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        worker.execute {
            try { NativeAppUpdates.installerResult(context.applicationContext, intent) }
            catch (_: Exception) { /* Durable previous state remains unknown; never convert a bad receipt into success. */ }
            finally { pending.finish() }
        }
    }
    companion object { private val worker = Executors.newSingleThreadExecutor() }
}

/** Version confirmation also runs when Android replaces this package and the UI is absent. */
class NativeAppUpdateReplacedReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        val pending = goAsync()
        Thread {
            try { synchronized(NativeAppUpdates.lock) { NativeAppUpdates.reconcile(context.applicationContext) } }
            catch (_: Exception) { }
            finally { pending.finish() }
        }.start()
    }
}
