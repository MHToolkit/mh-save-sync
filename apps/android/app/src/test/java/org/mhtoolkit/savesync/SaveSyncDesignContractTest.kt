package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveSyncDesignContractTest {
    @Test
    fun `design contract exposes semantic version and bounded response`() {
        assertEquals("apple-design-v1", SaveSyncDesignTokens.contractVersion)
        assertTrue(SaveSyncDesignTokens.statusMotionMillis in 120..240)
    }
}
