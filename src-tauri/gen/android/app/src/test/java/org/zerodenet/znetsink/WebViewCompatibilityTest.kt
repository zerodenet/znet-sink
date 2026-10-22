package org.zerodenet.znetsink

import org.junit.Assert.*
import org.junit.Test

class WebViewCompatibilityTest {
  @Test fun oldAndroidDoesNotDetermineEngineSupport() {
    assertTrue(WebViewCompatibility.supported("Mozilla/5.0 (Linux; Android 7.0; wv) Chrome/119.0.6045.163 Mobile Safari/537.36"))
    assertFalse(WebViewCompatibility.supported("Mozilla/5.0 (Linux; Android 14; wv) Chrome/110.0.0.0 Mobile Safari/537.36"))
  }
  @Test fun chromeFloorAndChromiumAreExplicit() {
    assertFalse(WebViewCompatibility.supported("Chrome/110.0.0.0"))
    assertTrue(WebViewCompatibility.supported("Chrome/111.0.0.0"))
    assertTrue(WebViewCompatibility.supported("Chromium/112.0.0.0"))
  }
  @Test fun absentMalformedAndOverflowVersionRemainUnconfirmed() {
    for (agent in listOf("Android/24 AppleWebKit/537.36", "OtherChrome/119.0", "Chrome/x", "Chrome/999999999999999999.0")) {
      assertNull(WebViewCompatibility.chromiumMajor(agent))
      assertFalse(WebViewCompatibility.supported(agent))
    }
  }
}
