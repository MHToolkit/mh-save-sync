package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveSyncDesignContractTest {
    @Test
    fun `design contract exposes a versioned bounded response`() {
        assertEquals("apple-design-v1", SaveSyncDesignTokens.contractVersion)
        assertTrue(SaveSyncDesignTokens.contentMotionMillis in 180..300)
    }
}
