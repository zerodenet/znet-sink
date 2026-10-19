package org.zerodenet.znetsink.nativeio

import java.io.ByteArrayOutputStream
import java.io.InputStream

/** Pure boundary checks, shared by commands and callbacks. No platform IO here. */
internal object NativeIoPolicy {
    const val SHARE_TTL_MS = 10 * 60 * 1000L
    private val operationPattern = Regex("[A-Za-z0-9_-]{1,64}")
    private val leafPattern = Regex("[A-Za-z0-9][A-Za-z0-9._ -]{0,127}")

    fun validOperation(value: String) = operationPattern.matches(value)

    fun shareExpired(created: Long, now: Long): Boolean =
        created <= 0 || now < created || now - created >= SHARE_TTL_MS

    fun validate(
        action: String, purpose: String, maximumBytes: Long, fileName: String?,
        mimeType: String, payloadBase64: String?, credentialsAcknowledged: Boolean,
    ): String? {
        val limit = when (purpose) {
            "configuration-import", "plugin-package-import", "ordinary-configuration-export" -> 16 * 1024 * 1024L
            "settings-import", "settings-export" -> 2 * 1024 * 1024L
            "redacted-diagnostic-export" -> 10 * 1024 * 1024L
            "plugin-visible-file-import", "plugin-visible-file-export" -> 4 * 1024 * 1024L
            else -> return "unsupported-purpose"
        }
        val importing = purpose.endsWith("-import")
        if ((action == "pickRead") != importing || action !in setOf("pickRead", "save", "share")) {
            return "purpose-action-mismatch"
        }
        if (purpose == "plugin-visible-file-export" && action != "save") return "purpose-action-mismatch"
        if (maximumBytes !in 1..limit) return "invalid-byte-budget"
        val allowedMimes = when (purpose) {
            "redacted-diagnostic-export" -> setOf("application/json", "text/plain", "application/zip")
            "plugin-visible-file-export" -> setOf("application/json", "text/plain", "application/zip", "application/octet-stream")
            else -> setOf("application/json")
        }
        if (mimeType !in allowedMimes) return "unsupported-mime-type"
        if (fileName != null && !leafPattern.matches(fileName)) return "invalid-file-name"
        if (importing) {
            if (payloadBase64 != null || credentialsAcknowledged) return "invalid-import-arguments"
        } else {
            if (fileName == null) return "invalid-file-name"
            if (purpose in setOf("ordinary-configuration-export", "plugin-visible-file-export") && !credentialsAcknowledged) {
                return "credentials-acknowledgement-required"
            }
            if (payloadBase64 == null) return "missing-payload"
            // Reject excessive encoded length before scanning/decoding any payload.
            if (payloadBase64.length.toLong() > ((maximumBytes + 2) / 3) * 4) {
                return "byte-budget-exceeded"
            }
            val padding = if (payloadBase64.endsWith("==")) 2 else if (payloadBase64.endsWith("=")) 1 else 0
            if (payloadBase64.length % 4 != 0 ||
                !payloadBase64.dropLast(padding).all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '+' || it == '/' }) {
                return "invalid-base64"
            }
            if (payloadBase64.length.toLong() / 4 * 3 - padding > maximumBytes) {
                return "byte-budget-exceeded"
            }
        }
        return null
    }

    /** Read at most max+1 bytes; overflow never returns partial content. */
    fun readBounded(input: InputStream, maximumBytes: Long): ByteArray {
        require(maximumBytes in 1..16 * 1024 * 1024L)
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(8192)
        while (true) {
            val count = input.read(buffer, 0, minOf(buffer.size.toLong(), maximumBytes + 1 - output.size()).toInt())
            if (count == -1) return output.toByteArray()
            if (count == 0) {
                val single = input.read()
                if (single == -1) return output.toByteArray()
                output.write(single)
            } else output.write(buffer, 0, count)
            if (output.size() > maximumBytes) throw ByteBudgetExceeded()
        }
    }
}

internal class ByteBudgetExceeded : Exception()
