package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DurableSyncPipelineTest {
    @Test
    fun `periodic work only captures an already dirty stopped session`() {
        assertFalse(AutomaticCapturePolicy.shouldCapture("periodic", dirty = false, sessionActive = false))
        assertTrue(AutomaticCapturePolicy.shouldCapture("periodic", dirty = true, sessionActive = false))
        assertFalse(AutomaticCapturePolicy.shouldCapture("periodic", dirty = true, sessionActive = true))
    }

    @Test
    fun `session exit and explicit save complete create candidates but never restore`() {
        assertTrue(AutomaticCapturePolicy.shouldCapture("session-exit", dirty = true, sessionActive = false))
        assertTrue(AutomaticCapturePolicy.shouldCapture("save-complete", dirty = true, sessionActive = true))
        assertFalse(AutomaticCapturePolicy.shouldCapture("dirty-observed", dirty = true, sessionActive = false))
    }

    @Test
    fun `failed native drain remains pending and requests retry`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":0,"conflict_count":0,"failed_count":1,"pending_count":2,"last_error":"network_or_server_failure"}""",
        )
        assertEquals(2, result.pendingCount)
        assertEquals(1, result.failedCount)
        assertTrue(result.shouldRetry)
    }

    @Test
    fun `successful native drain clears pending jobs`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":2,"conflict_count":0,"failed_count":0,"pending_count":0,"last_snapshot_id":"abc","last_cloud_head":"abc"}""",
        )
        assertEquals(2, result.uploadedCount)
        assertFalse(result.shouldRetry)
    }

    @Test
    fun `remaining fifo batch requests another worker even without a network error`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":100,"conflict_count":0,"failed_count":0,"pending_count":1}""",
        )
        assertTrue(result.shouldRetry)
    }
}
