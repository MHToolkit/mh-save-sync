package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class SyncConsistencyPolicyTest {
    private val binding = SyncConsistencyBinding(
        serverEndpoint = "https://save.example.test",
        logicalSaveId = "mh3g",
        treeUri = "content://save/tree/mh3g",
        deviceId = "android-test",
    )

    private fun baseline(
        head: String = "head-a",
        fingerprint: String = "fingerprint-a",
        mode: SyncEstablishmentMode = SyncEstablishmentMode.UPLOAD,
    ) = SyncConsistencyBaseline(
        binding = binding,
        establishedRemoteHead = head,
        localFingerprint = fingerprint,
        establishedAtMillis = 1234L,
        mode = mode,
    )

    private fun classify(
        stored: SyncConsistencyBaseline? = baseline(),
        local: LocalFingerprintObservation = LocalFingerprintObservation.Available("fingerprint-a"),
        remote: String? = "head-a",
        currentBinding: SyncConsistencyBinding = binding,
        emulatorRunning: Boolean = false,
    ) = PrelaunchConsistencyPolicy.classify(
        binding = currentBinding,
        baseline = stored,
        localFingerprint = local,
        remoteHead = remote,
        emulatorRunning = emulatorRunning,
    )

    @Test fun `own successful upload is synced on next prelaunch`() {
        assertEquals(PrelaunchConsistencyState.SYNCED, classify())
    }

    @Test fun `successful restore is synced on next prelaunch`() {
        assertEquals(
            PrelaunchConsistencyState.SYNCED,
            classify(stored = baseline(mode = SyncEstablishmentMode.RESTORE)),
        )
    }

    @Test fun `local mutation with unchanged cloud asks for upload`() {
        assertEquals(
            PrelaunchConsistencyState.LOCAL_CHANGED,
            classify(local = LocalFingerprintObservation.Available("fingerprint-b")),
        )
    }

    @Test fun `remote advance with unchanged local asks for restore`() {
        assertEquals(PrelaunchConsistencyState.REMOTE_ADVANCED, classify(remote = "head-b"))
    }

    @Test fun `both sides changing from established version is divergence`() {
        assertEquals(
            PrelaunchConsistencyState.DIVERGED,
            classify(
                local = LocalFingerprintObservation.Available("fingerprint-b"),
                remote = "head-b",
            ),
        )
    }

    @Test fun `first run with cloud data is neutral unknown not conflict`() {
        val state = classify(stored = null)
        assertEquals(PrelaunchConsistencyState.UNKNOWN, state)
        val copy = DashboardContentPolicy.launchStatus(state.reason)
        assertFalse(copy.contains("冲突"))
        assertTrue(copy.contains("选择"))
    }

    @Test fun `ledger is invalid when endpoint logical save tree or device binding changes`() {
        val changedBindings = listOf(
            binding.copy(serverEndpoint = "https://other.example.test"),
            binding.copy(logicalSaveId = "mh3u"),
            binding.copy(treeUri = "content://save/tree/other"),
            binding.copy(deviceId = "android-other"),
        )
        changedBindings.forEach { changed ->
            assertEquals(PrelaunchConsistencyState.UNKNOWN, classify(currentBinding = changed))
        }
    }

    @Test fun `SAF read failure fails closed without claiming conflict`() {
        val state = classify(local = LocalFingerprintObservation.Unavailable)
        assertEquals(PrelaunchConsistencyState.LOCAL_UNAVAILABLE, state)
        assertFalse(DashboardContentPolicy.launchStatus(state.reason).contains("冲突"))
        assertTrue(PrelaunchLaunchPolicy.allowExplicitLocalLaunch(state))
        assertFalse(PrelaunchLaunchPolicy.launchAutomatically(state))
    }

    @Test fun `running emulator never permits automatic launch or restore direction`() {
        val state = classify(emulatorRunning = true)
        assertEquals(PrelaunchConsistencyState.EMULATOR_RUNNING, state)
        assertFalse(PrelaunchCapturePolicy.shouldCaptureLocal(emulatorRunning = true))
        assertFalse(PrelaunchLaunchPolicy.launchAutomatically(state))
        assertFalse(PrelaunchLaunchPolicy.allowCloudRestore(state))
    }

    @Test fun `only verified synced state launches automatically`() {
        PrelaunchConsistencyState.entries.forEach { state ->
            assertEquals(
                state == PrelaunchConsistencyState.SYNCED || state == PrelaunchConsistencyState.NO_REMOTE,
                PrelaunchLaunchPolicy.launchAutomatically(state),
            )
        }
    }

    @Test fun `ledger codec rejects partial state instead of trusting stale head`() {
        val original = baseline()
        assertEquals(original, SyncConsistencyLedgerCodec.decode(SyncConsistencyLedgerCodec.encode(original)))
        assertEquals(null, SyncConsistencyLedgerCodec.decode("""{"established_remote_head":"head-a"}"""))
        assertEquals(null, SyncConsistencyLedgerCodec.decode("not-json"))
    }

    @Test fun `restore establishes consistency while lease is held then releases`() = runBlocking {
        val events = mutableListOf<String>()
        val established = RestoreConsistencyCoordinator.complete(
            captureAndEstablish = { events += "capture-and-establish" },
            releaseLease = { events += "release" },
        )
        assertTrue(established)
        assertEquals(listOf("capture-and-establish", "release"), events)
    }

    @Test fun `restore baseline failure still releases lease and reports not established`() = runBlocking {
        val events = mutableListOf<String>()
        val established = RestoreConsistencyCoordinator.complete(
            captureAndEstablish = {
                events += "capture-failed"
                error("SAF unavailable")
            },
            releaseLease = { events += "release" },
        )
        assertFalse(established)
        assertEquals(listOf("capture-failed", "release"), events)
    }

    @Test fun `restore cancellation releases lease then propagates cancellation`() {
        val events = mutableListOf<String>()
        assertThrows(CancellationException::class.java) {
            runBlocking {
                RestoreConsistencyCoordinator.complete(
                    captureAndEstablish = {
                        events += "cancelled"
                        throw CancellationException("cancel")
                    },
                    releaseLease = { events += "release" },
                )
            }
        }
        assertEquals(listOf("cancelled", "release"), events)
    }

    @Test fun `prelaunch refetches cloud head after stable SAF capture`() = runBlocking {
        val events = mutableListOf<String>()
        val observed = PrelaunchObservationCoordinator.captureThenRefetch(
            captureLocal = {
                events += "capture"
                "fingerprint-a"
            },
            refetchRemoteHead = {
                events += "refetch"
                "head-b"
            },
        )
        assertEquals(listOf("capture", "refetch"), events)
        assertEquals("fingerprint-a", observed.localFingerprint)
        assertEquals("head-b", observed.remoteHead)
        assertEquals(
            PrelaunchConsistencyState.REMOTE_ADVANCED,
            classify(remote = observed.remoteHead),
        )
    }
}
