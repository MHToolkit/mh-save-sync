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

    @Test
    fun `reduced motion removes layout animation instead of slowing the write gate`() {
        assertEquals(0, SaveSyncDesignTokens.motionDurationMillis(reducedMotion = true))
        assertEquals(
            SaveSyncDesignTokens.contentMotionMillis,
            SaveSyncDesignTokens.motionDurationMillis(reducedMotion = false),
        )
    }

    @Test
    fun `status rail uses full width steps for large font scales`() {
        assertEquals(listOf("检查", "确认", "写入/回滚"), SaveSyncDesignTokens.statusRailStepLabels)
        assertEquals(
            listOf("full-width-column", "live-region-summary"),
            SaveSyncDesignTokens.statusRailLayoutFallbacks,
        )
    }
}
