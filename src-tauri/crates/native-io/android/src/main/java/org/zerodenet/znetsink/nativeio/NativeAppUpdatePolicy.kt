package org.zerodenet.znetsink.nativeio

import java.net.URI
import java.io.InputStream
import java.io.OutputStream

/** Pure policy shared by the installer and local JVM tests. Facts come from APK/PackageManager. */
internal object NativeAppUpdatePolicy {
    const val MAX_BYTES = 512L * 1024 * 1024
    const val MAX_AGE_MS = 48L * 60 * 60 * 1000
    private val redirects = setOf("github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com")
    fun operation(value: String) = value.matches(Regex("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"))
    fun digest(value: String) = value.matches(Regex("[0-9a-f]{64}"))
    fun asset(url: String, name: String, version: String, size: Long): Boolean {
        if (size !in 1..MAX_BYTES || name.length > 180 || !name.matches(Regex("[A-Za-z0-9._-]+\\.apk")) || !version.matches(Regex("[0-9A-Za-z.+-]{1,256}"))) return false
        return try {
            val u = URI(url)
            u.scheme == "https" && u.host == "github.com" && u.port == -1 && u.userInfo == null && u.fragment == null && u.query == null &&
                (u.rawPath == "/zerodenet/znet-sink/releases/download/$version/$name" || u.rawPath == "/zerodenet/znet-sink/releases/download/v$version/$name")
        } catch (_: Exception) { false }
    }
    fun redirect(url: String): Boolean = try {
        val u = URI(url)
        u.scheme == "https" && u.host in redirects && (u.port == -1 || u.port == 443) && u.userInfo == null && u.fragment == null
    } catch (_: Exception) { false }

    fun transfer(input: InputStream, output: OutputStream, expectedSize: Long, active: () -> Unit, progress: (Long) -> Unit, elapsedNanos: () -> Long): Long {
        check(expectedSize in 1..MAX_BYTES) { "download-size-budget" }
        val buffer = ByteArray(64 * 1024)
        var received = 0L
        while (true) {
            active()
            check(elapsedNanos() <= 10L * 60 * 1_000_000_000) { "download-attempt-timeout" }
            val count = input.read(buffer)
            active()
            if (count < 0) break
            check(received + count <= expectedSize && received + count <= MAX_BYTES) { "download-size-budget" }
            output.write(buffer, 0, count); received += count
            progress(received)
        }
        check(received == expectedSize) { "download-truncated" }
        return received
    }

    data class Facts(val packageName: String, val version: String, val code: Long, val minSdk: Int, val signers: Set<String>, val abis: Set<String>)
    fun rejection(installed: Facts, candidate: Facts, expectedVersion: String, sdk: Int, deviceAbis: Set<String>): String? = when {
        candidate.packageName != installed.packageName -> "package-mismatch"
        candidate.code <= installed.code -> "version-not-newer"
        candidate.version != expectedVersion -> "version-name-mismatch"
        candidate.minSdk > sdk || candidate.minSdk < 1 -> "sdk-incompatible"
        candidate.signers.isEmpty() || installed.signers.isEmpty() || candidate.signers != installed.signers -> "signer-mismatch"
        candidate.abis.isNotEmpty() && candidate.abis.intersect(deviceAbis).isEmpty() -> "abi-incompatible"
        else -> null
    }
    fun confirms(installed: Facts, candidate: Facts): Boolean = installed.packageName == candidate.packageName && installed.code == candidate.code && installed.version == candidate.version && installed.signers.isNotEmpty() && installed.signers == candidate.signers
}
