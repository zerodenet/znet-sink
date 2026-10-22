package org.zerodenet.znetsink.nativeio

import android.app.Activity
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.StatFs
import android.provider.Settings
import android.util.AtomicFile
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.zip.ZipFile

/** App-private durable single-flight update owner. Neither a WebView nor Activity owns the job. */
object NativeAppUpdates {
    internal const val JOB_ID = 0x5a4e5501
    internal val lock = Any()
    private val worker = Executors.newSingleThreadExecutor()
    private val activeStates = setOf("queued", "downloading", "paused", "verifying", "ready", "preparing-install", "awaiting-user", "installing", "unknown")
    private const val INSTALL_ACTION = "org.zerodenet.znetsink.APP_UPDATE_RESULT"
    private val keys = setOf("action", "operationId", "assetUrl", "assetName", "sha256", "sizeBytes", "version")

    fun command(activity: Activity, invoke: Invoke) {
        val raw = invoke.getArgs()
        try {
            require(raw.keys().asSequence().all { it in keys })
            keys.forEach { require(!raw.has(it) || raw.isNull(it) || raw.opt(it) is String) }
            val action = raw.getString("action")
            require(action in setOf("status", "download", "install", "confirm", "cancel", "settings", "store"))
            if (action in setOf("settings", "store", "confirm")) {
                activity.runOnUiThread {
                    try {
                        require(!activity.isFinishing && !activity.isDestroyed && activity.hasWindowFocus())
                        synchronized(lock) {
                            val current = load(activity)
                            when (action) {
                                "settings" -> {
                                    // Open OS UI only after the user's explicit tap. Never change this setting.
                                    if (Build.VERSION.SDK_INT >= 26) activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")))
                                    else activity.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS))
                                }
                                "store" -> activity.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=${activity.packageName}")))
                                "confirm" -> {
                                    scoped(current, raw)
                                    require(current!!.getString("state") == "awaiting-user")
                                    val confirmation = confirmation(activity, current) ?: error("confirmation-unavailable")
                                    // The system-held PendingIntent survives Activity/process recreation. Its target is never recreated from page data.
                                    activity.startIntentSenderForResult(confirmation.intentSender, 0x5a55, null, 0, 0, 0)
                                    current.put("reason", "system-confirmation-opened"); save(activity, current)
                                }
                            }
                        }
                        invoke.resolve(snapshot(activity))
                    } catch (_: Exception) { invoke.reject("app-update-user-action-unavailable") }
                }
                return
            }
            worker.execute {
                try {
                    synchronized(lock) {
                        when (action) {
                            "status" -> reconcile(activity)
                            "download" -> begin(activity, raw)
                            "cancel" -> cancel(activity, raw)
                            "install" -> install(activity, raw)
                        }
                    }
                    invoke.resolve(snapshot(activity))
                } catch (_: Exception) { invoke.reject("app-update-request-rejected-or-unknown-read-status") }
            }
        } catch (_: Exception) { invoke.reject("invalid-app-update-request") }
    }

    fun resume(context: Context) {
        worker.execute { try { synchronized(lock) { reconcile(context) } } catch (_: Exception) { /* Explicit status reports unavailable; never reset a corrupt journal. */ } }
    }
    private fun directory(context: Context): File = File(context.noBackupFilesDir, "app-update").apply { check(exists() || mkdirs()) }
    internal fun apk(context: Context, id: String) = File(directory(context), "$id.apk")
    internal fun partial(context: Context, id: String) = File(directory(context), "$id.partial")
    private fun journal(context: Context) = AtomicFile(File(directory(context), "operation.json"))
    internal fun load(context: Context): JSONObject? {
        val file = journal(context)
        if (!file.baseFile.exists() && !File(file.baseFile.path + ".bak").exists()) return null
        // AtomicFile can restore .bak during openRead, so checking only baseFile.length is insufficient.
        val bytes = file.openRead().use { NativeIoPolicy.readBounded(it, 64 * 1024L) }
        val raw = JSONObject(String(bytes, Charsets.UTF_8))
        check(raw.getInt("schema") == 1 && NativeAppUpdatePolicy.operation(raw.getString("operationId")))
        check(raw.getString("state") in activeStates || raw.getString("state") in setOf("failed", "cancelled", "installed-confirmed"))
        check(NativeAppUpdatePolicy.digest(raw.getString("sha256")))
        check(NativeAppUpdatePolicy.asset(raw.getString("assetUrl"), raw.getString("assetName"), raw.getString("version"), raw.getLong("size")))
        check(raw.getLong("createdAt") > 0 && raw.getInt("attempts") in 0..8 && raw.getLong("downloaded") in 0..raw.getLong("size"))
        val seen = raw.getJSONArray("seen")
        check(seen.length() <= 128)
        val identities = (0 until seen.length()).map { seen.getString(it) }
        check(identities.all { NativeAppUpdatePolicy.operation(it) } && identities.toSet().size == identities.size)
        if (raw.has("versionCode")) {
            check(raw.getLong("versionCode") > 0 && raw.getString("packageName") == context.packageName && raw.getInt("minSdk") >= 1)
            val signers = raw.getJSONArray("signers")
            check(signers.length() in 1..8 && (0 until signers.length()).all { NativeAppUpdatePolicy.digest(signers.getString(it)) })
        }
        if (raw.has("sessionId")) check(raw.getInt("sessionId") >= 0 && NativeAppUpdatePolicy.operation(raw.getString("callbackToken")))
        if (raw.has("confirmationIdentity")) check(raw.getString("confirmationIdentity").length <= 8192 && raw.has("sessionId"))
        return raw
    }
    internal fun save(context: Context, value: JSONObject) {
        val file = journal(context)
        value.put("updatedAt", System.currentTimeMillis())
        val bytes = value.toString().toByteArray(Charsets.UTF_8)
        check(bytes.size <= 64 * 1024)
        val stream = file.startWrite()
        try { stream.write(bytes); file.finishWrite(stream) } catch (e: Exception) { file.failWrite(stream); throw e }
    }
    private fun scoped(current: JSONObject?, raw: JSONObject) { require(current != null && current.getString("operationId") == raw.getString("operationId")) }
    private fun begin(context: Context, raw: JSONObject) {
        require(!storeManaged(context))
        val id = raw.getString("operationId")
        require(NativeAppUpdatePolicy.operation(id) && NativeAppUpdatePolicy.digest(raw.getString("sha256")))
        val size = raw.getString("sizeBytes").toLong()
        require(NativeAppUpdatePolicy.asset(raw.getString("assetUrl"), raw.getString("assetName"), raw.getString("version"), size))
        val old = load(context)
        if (old != null && old.getString("operationId") == id) {
            require(old.getString("sha256") == raw.getString("sha256") && old.getString("assetUrl") == raw.getString("assetUrl"))
            return // Receipt re-read only. Never schedule a duplicate, including after cancellation/failure.
        }
        require(old == null || old.getString("state") !in activeStates)
        val seen = old?.optJSONArray("seen") ?: org.json.JSONArray()
        require((0 until seen.length()).none { seen.getString(it) == id })
        old?.getString("operationId")?.let { seen.put(it) }
        while (seen.length() > 128) seen.remove(0)
        val current = JSONObject().put("schema", 1).put("operationId", id).put("sha256", raw.getString("sha256"))
            .put("assetUrl", raw.getString("assetUrl")).put("assetName", raw.getString("assetName"))
            .put("version", raw.getString("version")).put("size", size).put("state", "queued")
            .put("downloaded", 0).put("createdAt", System.currentTimeMillis()).put("attempts", 0).put("seen", seen)
        save(context, current)
        old?.getString("operationId")?.let { apk(context, it).delete(); partial(context, it).delete() }
        val scheduler = context.getSystemService(Context.JOB_SCHEDULER_SERVICE) as JobScheduler
        val builder = JobInfo.Builder(JOB_ID, ComponentName(context, NativeAppUpdateDownloadJob::class.java))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setPersisted(true)
            .setBackoffCriteria(30_000, JobInfo.BACKOFF_POLICY_EXPONENTIAL)
        if (Build.VERSION.SDK_INT >= 28) builder.setEstimatedNetworkBytes(size, 0)
        if (scheduler.schedule(builder.build()) != JobScheduler.RESULT_SUCCESS) {
            current.put("state", "failed").put("reason", "os-job-rejected"); save(context, current)
        }
    }
    private fun cancel(context: Context, raw: JSONObject) {
        val current = load(context); scoped(current, raw); current!!
        require(current.getString("state") != "installed-confirmed")
        (context.getSystemService(Context.JOB_SCHEDULER_SERVICE) as JobScheduler).cancel(JOB_ID)
        val session = current.optInt("sessionId", -1)
        var abandoned = session < 0
        if (session >= 0) {
            try { context.packageManager.packageInstaller.abandonSession(session); abandoned = true }
            catch (_: Exception) { /* A committed installation may already be completing; reconciliation is authoritative. */ }
        }
        confirmation(context, current)?.cancel()
        current.put("state", if (abandoned) "cancelled" else "unknown").put("reason", if (abandoned) "user-cancelled" else "cancellation-result-unknown")
        save(context, current)
        apk(context, current.getString("operationId")).delete(); partial(context, current.getString("operationId")).delete()
        reconcile(context)
    }
    private fun permission(context: Context) = if (Build.VERSION.SDK_INT < 26) "system-confirmation" else if (context.packageManager.canRequestPackageInstalls()) "allowed" else "blocked"
    @Suppress("DEPRECATION")
    private fun storeManaged(context: Context): Boolean = context.packageManager.getInstallerPackageName(context.packageName) == "com.android.vending"
    @Suppress("DEPRECATION")
    private fun info(context: Context, path: String? = null): PackageInfo {
        val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
        return if (path == null) context.packageManager.getPackageInfo(context.packageName, flags)
            else context.packageManager.getPackageArchiveInfo(path, flags) ?: error("invalid-apk")
    }
    @Suppress("DEPRECATION")
    private fun facts(info: PackageInfo, abis: Set<String> = emptySet()): NativeAppUpdatePolicy.Facts {
        val signers = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
        return NativeAppUpdatePolicy.Facts(info.packageName, info.versionName ?: "", if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong(), info.applicationInfo?.minSdkVersion ?: 0,
            signers?.map { hex(MessageDigest.getInstance("SHA-256").digest(it.toByteArray())) }?.toSet() ?: emptySet(), abis)
    }
    private fun target(current: JSONObject) = NativeAppUpdatePolicy.Facts(current.getString("packageName"), current.getString("version"), current.getLong("versionCode"), current.getInt("minSdk"), current.getJSONArray("signers").let { array -> (0 until array.length()).map { array.getString(it) }.toSet() }, emptySet())
    internal fun verify(context: Context, current: JSONObject, file: File) {
        check(file.length() == current.getLong("size")) { "size-mismatch" }
        val hash = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { stream -> val buffer = ByteArray(64 * 1024); while (true) { val n = stream.read(buffer); if (n < 0) break; hash.update(buffer, 0, n) } }
        check(hex(hash.digest()) == current.getString("sha256")) { "digest-mismatch" }
        val abis = mutableSetOf<String>()
        ZipFile(file).use { zip ->
            var entries = 0; var expanded = 0L
            zip.entries().asSequence().forEach { entry ->
                check(++entries <= 20_000 && entry.size >= 0) { "apk-entry-budget" }
                check(entry.size <= 2L * 1024 * 1024 * 1024 - expanded) { "apk-expanded-budget" }; expanded += entry.size
                if (entry.name.startsWith("lib/") && entry.name.endsWith(".so")) {
                    val parts = entry.name.split('/'); check(parts.size == 3 && parts[1].isNotEmpty()) { "invalid-native-library" }; abis.add(parts[1])
                }
            }
        }
        val candidate = facts(info(context, file.absolutePath), abis)
        val reason = NativeAppUpdatePolicy.rejection(facts(info(context)), candidate, current.getString("version"), Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS.toSet())
        check(reason == null) { reason ?: "invalid-apk" }
        current.put("packageName", candidate.packageName).put("versionCode", candidate.code).put("minSdk", candidate.minSdk).put("signers", org.json.JSONArray(candidate.signers.toList()))
    }
    private fun install(context: Context, raw: JSONObject) {
        val current = load(context); scoped(current, raw); current!!
        require(current.getString("state") == "ready" && !storeManaged(context))
        if (permission(context) == "blocked") { current.put("reason", "unknown-source-permission-required"); save(context, current); return }
        val file = apk(context, current.getString("operationId"))
        verify(context, current, file) // Revalidate after any time in the background and immediately before creating a session.
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(context.packageName); params.setSize(file.length())
        if (Build.VERSION.SDK_INT >= 31) params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)
        val installer = context.packageManager.packageInstaller
        current.put("state", "preparing-install").put("reason", JSONObject.NULL); save(context, current)
        var sessionId = -1
        try {
            sessionId = installer.createSession(params)
            current.put("sessionId", sessionId).put("callbackToken", java.util.UUID.randomUUID().toString()); save(context, current)
            installer.openSession(sessionId).use { session ->
                session.openWrite("base.apk", 0, file.length()).use { output -> file.inputStream().use { it.copyTo(output, 64 * 1024) }; session.fsync(output) }
                val intent = Intent(context, NativeAppUpdateReceiver::class.java).setAction(INSTALL_ACTION)
                    .setData(Uri.parse("znet-app-update://${current.getString("callbackToken")}/$sessionId"))
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                val callback = PendingIntent.getBroadcast(context, sessionId, intent, flags)
                // Persist before commit. After this point a missing reply must never trigger an automatic replay.
                current.put("state", "installing"); save(context, current)
                session.commit(callback.intentSender)
                current.put("commitAcknowledged", true); save(context, current)
            }
        } catch (_: Exception) {
            val committed = current.getString("state") == "installing"
            if (!committed && sessionId >= 0) try { installer.abandonSession(sessionId) } catch (_: Exception) { }
            current.put("state", if (committed) "unknown" else "failed").put("reason", if (committed) "commit-result-unknown" else "session-prepare-failed"); save(context, current)
        }
    }
    private fun confirmation(context: Context, current: JSONObject): PendingIntent? {
        val uri = current.optString("confirmationIdentity", "")
        if (uri.isEmpty()) return null
        return PendingIntent.getActivity(context, current.getInt("sessionId"), Intent.parseUri(uri, 0), PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE)
    }
    internal fun installerResult(context: Context, intent: Intent) {
        synchronized(lock) {
            val current = load(context) ?: return
            val sessionId = current.optInt("sessionId", -1)
            if (sessionId < 0 || !NativeAppUpdatePolicy.operation(current.optString("callbackToken"))) return
            if (intent.action != INSTALL_ACTION || intent.data?.toString() != "znet-app-update://${current.optString("callbackToken")}/$sessionId" || intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, -2) != sessionId) return
            if (current.getString("state") !in setOf("installing", "awaiting-user", "unknown")) return
            val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
            current.put("installerStatus", status)
            when (status) {
                PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                    @Suppress("DEPRECATION") val confirmation = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                    if (confirmation == null || confirmation.resolveActivity(context.packageManager) == null) {
                        current.put("state", "unknown").put("reason", "confirmation-intent-unavailable")
                    } else {
                        // OS retains the complete intent including Binder extras; only its filter identity is stored in our journal.
                        PendingIntent.getActivity(context, sessionId, confirmation, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
                        current.put("confirmationIdentity", confirmation.cloneFilter().toUri(0)).put("state", "awaiting-user").put("reason", "system-user-confirmation-required")
                    }
                }
                PackageInstaller.STATUS_SUCCESS -> current.put("state", "unknown").put("reason", "installed-version-awaiting-confirmation")
                PackageInstaller.STATUS_FAILURE_ABORTED -> current.put("state", "cancelled").put("reason", "system-user-cancelled-or-verification-aborted")
                else -> current.put("state", "failed").put("reason", when (status) {
                    PackageInstaller.STATUS_FAILURE_BLOCKED -> "system-install-blocked"
                    PackageInstaller.STATUS_FAILURE_CONFLICT -> "system-package-conflict"
                    PackageInstaller.STATUS_FAILURE_INCOMPATIBLE -> "system-package-incompatible"
                    PackageInstaller.STATUS_FAILURE_INVALID -> "system-package-invalid"
                    PackageInstaller.STATUS_FAILURE_STORAGE -> "system-storage-unavailable"
                    else -> "system-install-failed"
                })
            }
            save(context, current); reconcile(context)
        }
    }
    internal fun reconcile(context: Context) {
        val current = load(context) ?: return
        if (current.has("versionCode") && NativeAppUpdatePolicy.confirms(facts(info(context)), target(current))) {
            current.put("state", "installed-confirmed").put("reason", "installed-package-version-signer-confirmed"); save(context, current)
            apk(context, current.getString("operationId")).delete(); partial(context, current.getString("operationId")).delete()
            return
        }
        val state = current.getString("state")
        // Preparing holds this same lock until it completes. Observing its journal here means the owner died.
        if (state == "preparing-install") {
            current.put("state", "unknown").put("reason", "install-preparation-interrupted"); save(context, current)
        }
        if (state == "installing" && !current.optBoolean("commitAcknowledged", false)) {
            current.put("state", "unknown").put("reason", "commit-result-unknown"); save(context, current)
        }
        if (state == "installed-confirmed") { current.put("state", "unknown").put("reason", "installed-package-changed"); save(context, current) }
        if (state == "awaiting-user" && confirmation(context, current) == null) { current.put("state", "unknown").put("reason", "system-confirmation-lost-recheck-or-cancel"); save(context, current) }
        if (state in setOf("installing", "preparing-install", "unknown")) {
            val id = current.optInt("sessionId", -1)
            if (id < 0 || context.packageManager.packageInstaller.getSessionInfo(id) == null) {
                current.put("state", "unknown").put("reason", "session-missing-installed-version-unconfirmed"); save(context, current)
            }
        }
        if (state in setOf("queued", "downloading", "paused", "verifying")) {
            val job = (context.getSystemService(Context.JOB_SCHEDULER_SERVICE) as JobScheduler).getPendingJob(JOB_ID)
            if (job == null) { current.put("state", "unknown").put("reason", "os-download-job-missing"); save(context, current) }
        }
    }
    internal fun snapshot(context: Context): JSObject = synchronized(lock) {
        reconcile(context)
        val current = load(context)
        val installed = facts(info(context))
        JSObject().put("state", if (storeManaged(context)) "store-managed" else current?.getString("state") ?: "idle")
            .put("operationId", current?.optString("operationId") ?: JSONObject.NULL)
            .put("reason", current?.opt("reason") ?: JSONObject.NULL)
            .put("packageName", installed.packageName).put("installedVersion", installed.version).put("installedVersionCode", installed.code.toString())
            .put("targetVersion", current?.optString("version") ?: JSONObject.NULL).put("targetVersionCode", current?.takeIf { it.has("versionCode") }?.getLong("versionCode")?.toString() ?: JSONObject.NULL)
            .put("downloadedBytes", (current?.optLong("downloaded") ?: 0).toString()).put("sizeBytes", current?.getLong("size")?.toString() ?: JSONObject.NULL)
            .put("assetName", current?.optString("assetName") ?: JSONObject.NULL).put("sha256", current?.optString("sha256") ?: JSONObject.NULL)
            .put("installPermission", permission(context)).put("confirmationAvailable", current?.let { it.getString("state") == "awaiting-user" && confirmation(context, it) != null } ?: false)
            .put("installerStatus", current?.opt("installerStatus") ?: JSONObject.NULL) as JSObject
    }
    internal fun hasSpace(context: Context, size: Long) = StatFs(directory(context).absolutePath).availableBytes >= size * 2 + 16L * 1024 * 1024
    internal fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it.toInt() and 255) }
}
