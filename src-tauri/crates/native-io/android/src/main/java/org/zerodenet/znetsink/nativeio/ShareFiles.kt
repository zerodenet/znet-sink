package org.zerodenet.znetsink.nativeio

import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileNotFoundException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Native process-owned cleanup; no activity, webview or JavaScript timer owns a grant. */
internal object ShareFiles {
    val scheduler = ScheduledThreadPoolExecutor(1).apply { removeOnCancelPolicy = true }
    val selections = ConcurrentHashMap<String, () -> Unit>()
    private const val ROOT = "native-io-share"
    private const val MAX_FILES = 32
    private const val MAX_STAGED_BYTES = 64 * 1024 * 1024L

    fun root(context: Context) = File(context.cacheDir, ROOT)

    @Synchronized
    fun stage(context: Context, name: String, bytes: ByteArray): Pair<File, Uri> {
        cleanup(context)
        val root = root(context).apply { mkdirs() }
        val existing = root.listFiles().orEmpty()
        if (existing.size >= MAX_FILES || existing.sumOf { directory ->
                directory.listFiles().orEmpty().sumOf { it.length() }
            } + bytes.size > MAX_STAGED_BYTES) throw ByteBudgetExceeded()
        val directory = File(root, UUID.randomUUID().toString()).apply {
            if (!mkdir()) throw FileNotFoundException()
        }
        try {
            val file = File(directory, name)
            file.outputStream().use { it.write(bytes); it.flush() }
            directory.setLastModified(System.currentTimeMillis())
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.nativeio.files", file)
            val app = context.applicationContext
            scheduler.schedule({ remove(app, directory) }, NativeIoPolicy.SHARE_TTL_MS, TimeUnit.MILLISECONDS)
            return file to uri
        } catch (failure: Exception) {
            directory.deleteRecursively()
            throw failure
        }
    }

    fun expired(directory: File, now: Long = System.currentTimeMillis()): Boolean {
        return NativeIoPolicy.shareExpired(directory.lastModified(), now)
    }

    @Synchronized
    fun cleanup(context: Context) {
        root(context).listFiles().orEmpty().forEach { directory ->
            if (expired(directory)) remove(context, directory)
        }
    }

    @Synchronized
    fun remove(context: Context, directory: File) {
        directory.listFiles().orEmpty().forEach { file ->
            try {
                val uri = FileProvider.getUriForFile(context, "${context.packageName}.nativeio.files", file)
                context.revokeUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            } catch (_: Exception) { /* No URI, path, payload or exception text enters logs. */ }
        }
        directory.deleteRecursively()
    }
}

/** Even after a killed process restarts, an expired grant cannot open retained cache content. */
class NativeIoFileProvider : FileProvider() {
    override fun onCreate(): Boolean {
        val created = super.onCreate()
        context?.let { ShareFiles.cleanup(it) }
        return created
    }

    private fun checkLive(uri: Uri) {
        val context = context ?: throw FileNotFoundException("share-unavailable")
        val segments = uri.pathSegments
        if (segments.size != 3 || segments[0] != "native-io-share" ||
            !Regex("[a-f0-9-]{36}").matches(segments[1])) throw FileNotFoundException("share-unavailable")
        val root = ShareFiles.root(context).canonicalFile
        val directory = File(root, segments[1]).canonicalFile
        val file = File(directory, segments[2]).canonicalFile
        if (directory.parentFile != root || file.parentFile != directory || !file.isFile || ShareFiles.expired(directory)) {
            throw FileNotFoundException("share-expired")
        }
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        try {
            if (mode != "r") throw FileNotFoundException()
            checkLive(uri)
            return super.openFile(uri, mode) ?: throw FileNotFoundException()
        } catch (_: Exception) {
            // FileProvider/ParcelFileDescriptor exceptions can include a private absolute path.
            throw FileNotFoundException("share-unavailable")
        }
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor {
        try {
            checkLive(uri)
            return super.query(uri, projection, selection, selectionArgs, sortOrder)
        } catch (_: Exception) { throw IllegalArgumentException("share-unavailable") }
    }

    override fun getType(uri: Uri): String? {
        try {
            checkLive(uri)
            return super.getType(uri)
        } catch (_: Exception) { throw IllegalArgumentException("share-unavailable") }
    }
}

/** Only the OS's explicit PendingIntent can address this unexported receiver. */
class ShareSelectionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        try {
            if (intent.action != "${context.packageName}.nativeio.SHARE_SELECTED") return
            @Suppress("DEPRECATION")
            val chosen = intent.getParcelableExtra<ComponentName>(Intent.EXTRA_CHOSEN_COMPONENT) ?: return
            if (chosen.packageName.isEmpty() || chosen.className.isEmpty()) return
            val token = intent.data?.lastPathSegment ?: return
            ShareFiles.selections.remove(token)?.invoke()
        } catch (_: Exception) { /* Malformed callback is unconfirmed, with no content-bearing log. */ }
    }
}
