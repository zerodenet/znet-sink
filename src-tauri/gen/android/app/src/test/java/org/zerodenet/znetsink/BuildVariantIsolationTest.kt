package org.zerodenet.znetsink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Exercise the generated variant configuration, not a second policy constant. */
class BuildVariantIsolationTest {
  @Test fun debugAndReleaseHaveSeparateInstalledIdentities() {
    assertEquals(
      if (BuildConfig.DEBUG) "org.zerodenet.znetsink.debug" else "org.zerodenet.znetsink",
      BuildConfig.APPLICATION_ID,
    )
  }

  @Test fun developmentVersionSuffixCannotAppearInRelease() {
    if (BuildConfig.DEBUG) {
      assertTrue(BuildConfig.VERSION_NAME.endsWith("-debug"))
    } else {
      assertFalse(BuildConfig.VERSION_NAME.endsWith("-debug"))
    }
  }
}
