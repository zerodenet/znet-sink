package org.zerodenet.znetsink

/** Based on the active WebView's own UA, never the Android API level or an installed APK version. */
internal object WebViewCompatibility {
  const val MINIMUM_CHROMIUM_MAJOR = 111
  private val chromiumVersion = Regex("\\b(?:Chrome|Chromium)/(\\d+)(?:\\.|\\s|$)")

  fun chromiumMajor(userAgent: String): Int? =
    chromiumVersion.find(userAgent)?.groupValues?.get(1)?.toIntOrNull()

  fun supported(userAgent: String): Boolean =
    chromiumMajor(userAgent)?.let { it >= MINIMUM_CHROMIUM_MAJOR } ?: false
}
