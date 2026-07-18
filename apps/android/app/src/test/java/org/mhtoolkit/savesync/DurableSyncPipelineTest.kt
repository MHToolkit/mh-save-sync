package org.mhtoolkit.savesync

import androidx.work.NetworkType
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
    fun `session exit creates candidates but unavailable save complete ipc does not`() {
        assertTrue(AutomaticCapturePolicy.shouldCapture("session-exit", dirty = true, sessionActive = false))
        assertFalse(AutomaticCapturePolicy.shouldCapture("save-complete", dirty = true, sessionActive = true))
        assertFalse(SyncScheduler.SAVE_COMPLETE_IPC_AVAILABLE)
        assertFalse(SyncScheduler.PROCESS_EXIT_RUNTIME_VERIFIED)
        assertFalse(AutomaticCapturePolicy.shouldCapture("dirty-observed", dirty = true, sessionActive = false))
    }

    @Test
    fun `failed native drain remains pending and requests retry`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":0,"conflict_count":0,"failed_count":1,"pending_count":2,"pending_endpoint_count":2,"last_error":"network_or_server_failure","queue_state_known":true}""",
        )
        assertEquals(2, result.pendingCount)
        assertEquals(1, result.failedCount)
        assertTrue(result.shouldRetry)
        assertEquals(2, result.pendingEndpointCount)
    }

    @Test
    fun `capture claim parser distinguishes owned work from busy generation`() {
        assertEquals(
            CaptureGenerationClaim(7, "00112233445566778899aabbccddeeff"),
            CaptureGenerationClaim.parse(
                """{"claimed":true,"generation":7,"owner":"00112233445566778899aabbccddeeff"}""",
            ),
        )
        assertEquals(null, CaptureGenerationClaim.parse("""{"claimed":false}"""))
    }

    @Test
    fun `revoked saf grant fails closed`() {
        val root = "content://tree/nemessix"
        assertTrue(SafGrantPolicy.isUsable(root, setOf(root)))
        assertFalse(SafGrantPolicy.isUsable(root, emptySet()))
        assertFalse(SafGrantPolicy.isUsable(null, setOf(root)))
        assertEquals(
            SafGrantInspection.Revoked,
            SafGrantInspection.inspect { throw SecurityException("revoked") },
        )
    }

    @Test
    fun `session exit requires process to be observed before consecutive absence`() {
        var state = ProcessObservationState()
        repeat(10) { state = state.next(running = false) }
        assertFalse(state.confirmedExit(3))
        state = state.next(running = true)
        repeat(2) { state = state.next(running = false) }
        assertFalse(state.confirmedExit(3))
        state = state.next(running = false)
        assertTrue(state.confirmedExit(3))
    }

    @Test
    fun `session evidence is tied to exact nemessix package process`() {
        assertTrue(NemessixProcessEvidence.matches(SyncScheduler.NEMESSIX_PACKAGE, emptyList()))
        assertTrue(
            NemessixProcessEvidence.matches(
                "${SyncScheduler.NEMESSIX_PACKAGE}:gpu",
                emptyList(),
            ),
        )
        assertTrue(NemessixProcessEvidence.matches("app_process", listOf(SyncScheduler.NEMESSIX_PACKAGE)))
        assertFalse(NemessixProcessEvidence.matches("org.example.other", listOf("org.example.other")))
    }

    @Test
    fun `capture is offline capable while every drain is network constrained`() {
        val capture = SyncScheduler.captureConstraints(chargingRequired = false)
        assertEquals(NetworkType.NOT_REQUIRED, capture.requiredNetworkType)
        assertTrue(capture.requiresBatteryNotLow())

        val wifiDrain = SyncScheduler.drainConstraints(wifiOnly = true, chargingRequired = true)
        assertEquals(NetworkType.UNMETERED, wifiDrain.requiredNetworkType)
        assertTrue(wifiDrain.requiresBatteryNotLow())
        assertTrue(wifiDrain.requiresCharging())

        val anyNetworkDrain = SyncScheduler.drainConstraints(
            wifiOnly = false,
            chargingRequired = false,
        )
        assertEquals(NetworkType.CONNECTED, anyNetworkDrain.requiredNetworkType)
    }

    @Test
    fun `successful native drain clears pending jobs`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":2,"conflict_count":0,"failed_count":0,"pending_count":0,"last_snapshot_id":"abc","last_cloud_head":"abc","queue_state_known":true}""",
        )
        assertEquals(2, result.uploadedCount)
        assertFalse(result.shouldRetry)
    }

    @Test
    fun `remaining fifo batch requests another worker even without a network error`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":100,"conflict_count":0,"failed_count":0,"pending_count":1,"queue_state_known":true}""",
        )
        assertTrue(result.shouldRetry)
    }

    @Test
    fun `unknown queue state retries instead of reporting empty success`() {
        val result = DurableDrainResult.parse(
            """{"uploaded_count":0,"failed_count":1,"pending_count":0,"last_error":"local_queue_unavailable","queue_state_known":false}""",
        )
        assertTrue(result.shouldRetry)
        assertFalse(result.queueStateKnown)
    }
}
