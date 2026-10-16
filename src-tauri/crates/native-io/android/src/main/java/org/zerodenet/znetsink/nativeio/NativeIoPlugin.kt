package org.zerodenet.znetsink.nativeio

import android.app.Activity
import android.app.PendingIntent
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Base64
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets

@InvokeArg
class NativeIoArgs {
    var operationId: String = ""
    var purpose: String = ""
    var maximumBytes: Long = 0
    var fileName: String? = null
    var mimeType: String = "application/json"
    var payloadBase64: String? = null
    var credentialsAcknowledged: Boolean = false
}

/** Rust-only local plugin. Rust deliberately registers no JavaScript invoke handler. */
@TauriPlugin
class NativeIoPlugin(private val activity: Activity) : Plugin(activity) {
    private class Operation(val invoke: Invoke, val args: NativeIoArgs, val action: String) {
        val completed = AtomicBoolean(false)
        val awaitingSystemResult = AtomicBoolean(false)
        var payload: ByteArray? = null
        var shareToken: String? = null
        var selectionIntent: PendingIntent? = null
        var selectionTimeout: ScheduledFuture<*>? = null
    }
    private val active = AtomicReference<Operation?>(null)
    private val worker = Executors.newSingleThreadExecutor()
    private val destroyed = AtomicBoolean(false)
    private val readingLicenses = AtomicBoolean(false)

    override fun load(webView: WebView) {
        cleanStaging()
    }

    override fun onResume() {
        if (!destroyed.get()) cleanStaging()
    }

    override fun onDestroy(activity: AppCompatActivity) {
        destroyed.set(true)
        active.get()?.let { finish(it, "result-unknown", "activity-recreated") }
        worker.shutdown()
        // Shared staging is retained until expiry; recipients can still read accepted shares.
    }

    private fun cleanStaging() {
        try {
            worker.execute {
                try { ShareFiles.cleanup(activity.applicationContext) }
                catch (_: Exception) { /* Provider still checks expiration before opening content. */ }
            }
        } catch (_: RejectedExecutionException) { /* Activity has already been destroyed. */ }
    }

    private fun work(operation: Operation, action: () -> Unit) {
        try { worker.execute(action) }
        catch (_: RejectedExecutionException) { finish(operation, "result-unknown", "activity-recreated") }
    }

    @Command
    fun pickRead(invoke: Invoke) = begin(invoke, "pickRead")

    @Command
    fun save(invoke: Invoke) = begin(invoke, "save")

    @Command
    fun share(invoke: Invoke) = begin(invoke, "share")

    /** Fixed packaged asset, reachable only through the Rust plugin handle. No path argument. */
    @Command
    fun readLicenseInventory(invoke: Invoke) {
        try {
            if (invoke.getArgs().length() != 0) {
                licenseResult(invoke, "invalid", "unexpected-license-arguments")
                return
            }
        } catch (_: Exception) {
            licenseResult(invoke, "invalid", "invalid-license-arguments")
            return
        }
        if (destroyed.get() || !readingLicenses.compareAndSet(false, true)) {
            licenseResult(invoke, "invalid", "native-license-reader-unavailable")
            return
        }
        try {
            worker.execute {
                try {
                    if (activity.assets.list("")?.contains("license-inventory.native.json") == false) {
                        licenseResult(invoke, "missing", "native-license-inventory-not-packaged")
                        return@execute
                    }
                    val bytes = activity.assets.open("license-inventory.native.json").use {
                        NativeIoPolicy.readBounded(it, 16 * 1024 * 1024L)
                    }
                    val content = StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(bytes)).toString()
                    val document = JSObject(content)
                    if (document.opt("schemaVersion") != 1 || document.optJSONObject("target")?.optString("platform") != "android" ||
                        document.optJSONObject("provenance") == null || document.optJSONArray("entries") == null ||
                        document.optJSONArray("texts") == null || document.optJSONArray("gaps") == null) {
                        licenseResult(invoke, "invalid", "invalid-native-license-inventory")
                    } else licenseResult(invoke, "available", content = content)
                } catch (_: java.io.FileNotFoundException) {
                    licenseResult(invoke, "missing", "native-license-inventory-not-packaged")
                } catch (_: Exception) {
                    licenseResult(invoke, "invalid", "invalid-native-license-inventory")
                } finally { readingLicenses.set(false) }
            }
        } catch (_: RejectedExecutionException) {
            readingLicenses.set(false)
            licenseResult(invoke, "invalid", "native-license-reader-unavailable")
        }
    }

    private fun licenseResult(invoke: Invoke, status: String, reason: String? = null, content: String? = null) {
        invoke.resolve(JSObject().apply {
            put("status", status)
            if (reason != null) put("reason", reason)
            if (content != null) put("content", content)
        })
    }

    private fun begin(invoke: Invoke, action: String) {
        var operationId = ""
        val args: NativeIoArgs
        try {
            val raw = invoke.getArgs()
            operationId = (raw.opt("operationId") as? String).orEmpty()
            val allowed = setOf("operationId", "purpose", "maximumBytes", "fileName", "mimeType", "payloadBase64", "credentialsAcknowledged")
            if (raw.keys().asSequence().any { it !in allowed } ||
                raw.opt("operationId") !is String || raw.opt("purpose") !is String ||
                (raw.has("mimeType") && raw.opt("mimeType") !is String) ||
                (raw.opt("maximumBytes") !is Int && raw.opt("maximumBytes") !is Long) ||
                (raw.has("fileName") && raw.opt("fileName") !is String) ||
                (raw.has("payloadBase64") && raw.opt("payloadBase64") !is String) ||
                (raw.has("credentialsAcknowledged") && raw.opt("credentialsAcknowledged") !is Boolean)) {
                respond(invoke, operationId, "failed", "invalid-arguments")
                return
            }
            args = invoke.parseArgs(NativeIoArgs::class.java)
        } catch (_: Exception) {
            respond(invoke, operationId, "failed", "invalid-arguments")
            return
        }
        val error = if (!NativeIoPolicy.validOperation(args.operationId)) "invalid-operation-id" else
            NativeIoPolicy.validate(action, args.purpose, args.maximumBytes, args.fileName, args.mimeType,
                args.payloadBase64, args.credentialsAcknowledged)
        if (error != null) {
            respond(invoke, args.operationId, "failed", error)
            return
        }
        if (destroyed.get() || activity.isFinishing || activity.isDestroyed) {
            respond(invoke, args.operationId, "failed", "activity-unavailable")
            return
        }
        val operation = Operation(invoke, args, action)
        if (!active.compareAndSet(null, operation)) {
            respond(invoke, args.operationId, "failed", "native-io-busy")
            return
        }
        work(operation) {
            try {
                if (action != "pickRead") {
                    val bytes = Base64.decode(args.payloadBase64, Base64.NO_WRAP)
                    if (bytes.size > args.maximumBytes) throw ByteBudgetExceeded()
                    operation.payload = bytes
                    // Do not retain both the encoded and decoded bodies while awaiting the user.
                    args.payloadBase64 = null
                }
                if (operation.completed.get()) return@work
                when (action) {
                    "pickRead" -> launch(operation, Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = args.mimeType
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }, "pickReadResult")
                    "save" -> launch(operation, Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = args.mimeType
                        putExtra(Intent.EXTRA_TITLE, args.fileName)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
                    }, "saveResult")
                    "share" -> launchShare(operation)
                }
            } catch (_: ByteBudgetExceeded) {
                finish(operation, "failed", "byte-budget-exceeded")
            } catch (_: Exception) {
                finish(operation, "failed", "native-io-unavailable")
            }
        }
    }

    private fun launch(operation: Operation, intent: Intent, callback: String) {
        activity.runOnUiThread {
            if (operation.completed.get()) return@runOnUiThread
            try {
                operation.awaitingSystemResult.set(true)
                startActivityForResult(operation.invoke, intent, callback)
            } catch (_: Exception) {
                operation.awaitingSystemResult.set(false)
                finish(operation, "failed", "system-picker-unavailable")
            }
        }
    }

    @ActivityCallback
    fun pickReadResult(invoke: Invoke, result: ActivityResult) {
        val operation = matching(invoke, "pickRead") ?: return
        operation.awaitingSystemResult.set(false)
        if (result.resultCode == Activity.RESULT_CANCELED) {
            result.data?.data?.let { releaseDocument(it) }
            finish(operation, "cancelled")
            return
        }
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null || uri.scheme != "content") {
            uri?.let { releaseDocument(it) }
            finish(operation, "failed", "invalid-document-result")
            return
        }
        work(operation) {
            try {
                val bytes = activity.contentResolver.openInputStream(uri)?.use {
                    NativeIoPolicy.readBounded(it, operation.args.maximumBytes)
                } ?: throw java.io.FileNotFoundException()
                finish(operation, "read", bytes = bytes)
            } catch (_: ByteBudgetExceeded) { finish(operation, "failed", "byte-budget-exceeded") }
            catch (_: SecurityException) { finish(operation, "failed", "document-permission-denied") }
            catch (_: Exception) { finish(operation, "failed", "document-read-failed") }
            finally { releaseDocument(uri) }
        }
    }

    @ActivityCallback
    fun saveResult(invoke: Invoke, result: ActivityResult) {
        val operation = matching(invoke, "save") ?: return
        operation.awaitingSystemResult.set(false)
        if (result.resultCode == Activity.RESULT_CANCELED) {
            result.data?.data?.let { releaseDocument(it) }
            finish(operation, "cancelled")
            return
        }
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null || uri.scheme != "content") {
            uri?.let { releaseDocument(it) }
            finish(operation, "result-unknown", "invalid-document-result")
            return
        }
        work(operation) {
            try {
                val bytes = operation.payload ?: throw IllegalStateException()
                val stream = activity.contentResolver.openOutputStream(uri, "wt")
                    ?: throw java.io.FileNotFoundException()
                stream.use { it.write(bytes); it.flush() }
                try {
                    val reread = activity.contentResolver.openInputStream(uri)?.use {
                        NativeIoPolicy.readBounded(it, operation.args.maximumBytes)
                    }
                    if (reread != null && bytes.contentEquals(reread)) finish(operation, "saved")
                    else finish(operation, "result-unknown", "save-verification-unconfirmed")
                } catch (_: Exception) { finish(operation, "result-unknown", "save-verification-unconfirmed") }
            } catch (_: Exception) {
                // CREATE_DOCUMENT may already have created a file, even if opening its stream fails.
                finish(operation, "result-unknown", "document-write-unconfirmed")
            } finally { releaseDocument(uri) }
        }
    }

    private fun launchShare(operation: Operation) {
        val (file, uri) = ShareFiles.stage(activity.applicationContext, operation.args.fileName!!, operation.payload!!)
        operation.payload = null
        if (operation.completed.get()) {
            ShareFiles.remove(activity.applicationContext, file.parentFile!!)
            return
        }
        val token = UUID.randomUUID().toString()
        operation.shareToken = token
        ShareFiles.selections[token] = {
            if (!ShareFiles.expired(file.parentFile!!)) finish(operation, "share-accepted")
            else finish(operation, "result-unknown", "share-expired")
        }
        val callback = Intent(activity, ShareSelectionReceiver::class.java).apply {
            action = "${activity.packageName}.nativeio.SHARE_SELECTED"
            data = Uri.parse("nativeio://selection/$token")
        }
        val flags = PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_ONE_SHOT or
            (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
        val pendingIntent = PendingIntent.getBroadcast(activity, 0, callback, flags)
        operation.selectionIntent = pendingIntent
        if (operation.completed.get()) {
            ShareFiles.selections.remove(token)
            pendingIntent.cancel()
            ShareFiles.remove(activity.applicationContext, file.parentFile!!)
            return
        }
        val send = Intent(Intent.ACTION_SEND).apply {
            type = operation.args.mimeType
            putExtra(Intent.EXTRA_STREAM, uri)
            clipData = ClipData.newUri(activity.contentResolver, operation.args.fileName, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val chooser = Intent.createChooser(send, null, pendingIntent.intentSender).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        operation.selectionTimeout = ShareFiles.scheduler.schedule({ finish(operation, "result-unknown", "share-selection-unconfirmed") },
            NativeIoPolicy.SHARE_TTL_MS, TimeUnit.MILLISECONDS)
        launch(operation, chooser, "shareResult")
    }

    @ActivityCallback
    fun shareResult(invoke: Invoke, result: ActivityResult) {
        val operation = active.get()?.takeIf { it.invoke.id == invoke.id && it.action == "share" } ?: return
        operation.awaitingSystemResult.set(false)
        if (operation.completed.get()) {
            active.compareAndSet(operation, null)
            return
        }
        // ACTION_SEND targets and OEM choosers do not provide a dependable cancel/delivery result.
        // Give the chooser's separate broadcast a small native dispatch window, then report unknown.
        ShareFiles.scheduler.schedule({ finish(operation, "result-unknown", "share-selection-unconfirmed") },
            500, TimeUnit.MILLISECONDS)
    }

    private fun matching(invoke: Invoke, action: String): Operation? = active.get()?.takeIf {
        it.invoke.id == invoke.id && it.action == action && !it.completed.get()
    }

    private fun releaseDocument(uri: Uri) {
        // Never takePersistableUriPermission. Revoke our transient grant after IO completion.
        try { activity.revokeUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) }
        catch (_: Exception) { /* Permission cleanup has no content-bearing error output. */ }
    }

    private fun finish(operation: Operation, status: String, reason: String? = null, bytes: ByteArray? = null) {
        if (!operation.completed.compareAndSet(false, true)) return
        operation.shareToken?.let { ShareFiles.selections.remove(it) }
        operation.selectionIntent?.cancel()
        operation.selectionTimeout?.cancel(false)
        operation.payload = null
        operation.args.payloadBase64 = null
        if (!operation.awaitingSystemResult.get() || destroyed.get()) active.compareAndSet(operation, null)
        respond(operation.invoke, operation.args.operationId, status, reason, bytes)
    }

    private fun respond(invoke: Invoke, id: String, status: String, reason: String? = null, bytes: ByteArray? = null) {
        val response = JSObject().apply {
            put("operationId", if (NativeIoPolicy.validOperation(id)) id else "")
            put("status", status)
            if (reason != null) put("reason", reason)
            if (bytes != null) {
                put("bytesRead", bytes.size)
                put("contentBase64", Base64.encodeToString(bytes, Base64.NO_WRAP))
            }
        }
        // Never reject with an Exception: Tauri logs exception text, which can contain a URI.
        invoke.resolve(response)
    }
}
