package org.mhtoolkit.savesync

import java.nio.file.Files
import kotlin.io.path.writeText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class RestorePathPolicyTest {
    @Test
    fun acceptsOrdinaryNestedFixture() {
        val root = Files.createTempDirectory("restore-policy").toFile()
        root.resolve("slot").mkdirs()
        root.resolve("slot/system").writeText("fixture")
        assertEquals(listOf("slot", "slot/system"), RestorePathPolicy.listFiles(root).map { it.first })
        root.deleteRecursively()
    }

    @Test
    fun rejectsCaseCollision() {
        expectRejected { RestorePathPolicy.validatePaths(listOf("SAVE", "save")) }
    }

    @Test
    fun rejectsSymlink() {
        val root = Files.createTempDirectory("restore-policy-link")
        val outside = Files.createTempFile("restore-outside", ".bin").also { it.writeText("fixture") }
        Files.createSymbolicLink(root.resolve("link"), outside)
        expectRejected { RestorePathPolicy.listFiles(root.toFile()) }
        root.toFile().deleteRecursively()
        Files.deleteIfExists(outside)
    }

    @Test
    fun throwableDuringMutationRollsBackBeforeLeaseMayBeReleased() {
        val operation = Files.createTempDirectory("restore-operation").toFile()
        val desired = operation.resolve("incoming").apply { mkdirs(); resolve("save").writeText("new") }
        val backup = operation.resolve("before").apply { mkdirs(); resolve("save").writeText("old") }
        var live = "old"
        var calls = 0
        val dyingBackend = RestoreTreeBackend { source ->
            live = source.resolve("save").readText()
            calls++
            if (calls == 1) throw SimulatedProcessDeath()
        }
        expectRejected {
            RestoreTransaction(DurableRestoreState(operation), dyingBackend).commit(desired, backup)
        }
        assertEquals("old", live)
        assertEquals(RestoreOperationState.ROLLED_BACK, DurableRestoreState(operation).read())
        operation.deleteRecursively()
    }

    @Test
    fun rollbackFailureRemainsDurablyBlockedForRecovery() {
        val operation = Files.createTempDirectory("restore-rollback-failure").toFile()
        val desired = operation.resolve("incoming").apply { mkdirs(); resolve("save").writeText("new") }
        val backup = operation.resolve("before").apply { mkdirs(); resolve("save").writeText("old") }
        var calls = 0
        try {
            RestoreTransaction(DurableRestoreState(operation), RestoreTreeBackend {
                calls++
                throw IllegalStateException(if (calls == 1) "mutation_failed" else "rollback_failed")
            }).commit(desired, backup)
            fail("expected rollback failure")
        } catch (_: IllegalStateException) { }
        assertEquals(2, calls)
        assertEquals(RestoreOperationState.ROLLBACK_REQUIRED, DurableRestoreState(operation).read())
        operation.deleteRecursively()
    }

    @Test
    fun restartRecoversEveryNonTerminalMutationState() {
        for (interrupted in listOf(RestoreOperationState.MUTATING, RestoreOperationState.ROLLBACK_REQUIRED)) {
            val operation = Files.createTempDirectory("restore-restart-$interrupted").toFile()
            val backup = operation.resolve("before").apply { mkdirs(); resolve("save").writeText("old") }
            DurableRestoreState(operation).write(interrupted)
            var live = "partial"
            RestoreTransaction(DurableRestoreState(operation), RestoreTreeBackend { source ->
                live = source.resolve("save").readText()
            }).recover(backup)
            assertEquals("old", live)
            assertEquals(RestoreOperationState.ROLLED_BACK, DurableRestoreState(operation).read())
            operation.deleteRecursively()
        }
    }

    @Test
    fun ordinaryCommitFailureRollsBackImmediately() {
        val operation = Files.createTempDirectory("restore-rollback").toFile()
        val desired = operation.resolve("incoming").apply { mkdirs(); resolve("save").writeText("new") }
        val backup = operation.resolve("before").apply { mkdirs(); resolve("save").writeText("old") }
        var calls = 0
        var live = "old"
        expectRejected {
            RestoreTransaction(DurableRestoreState(operation), RestoreTreeBackend { source ->
                live = source.resolve("save").readText()
                calls++
                if (calls == 1) error("fixture write failure")
            }).commit(desired, backup)
        }
        assertEquals("old", live)
        assertEquals(RestoreOperationState.ROLLED_BACK, DurableRestoreState(operation).read())
        operation.deleteRecursively()
    }

    @Test
    fun stoppedGateRequiresFreshInactiveSessionConfirmation() {
        val now = 5_000L
        expectRejected { RestoreStopGate.requireFreshConfirmation(RestoreStopEvidence(now, false), now) }
        expectRejected { RestoreStopGate.requireFreshConfirmation(RestoreStopEvidence(0L, true), 130_000L) }
        RestoreStopGate.requireFreshConfirmation(RestoreStopEvidence(now, true), now)
    }

    @Test
    fun terminalReceiptMustMatchDurableStateAndExactManifest() {
        val challenge = "11".repeat(32)
        val desired = "22".repeat(32)
        val backup = "33".repeat(32)
        RestoreTerminalPolicy.requireReceiptMatches(
            RestoreOperationState.COMMITTED, desired, backup, challenge, "COMMITTED", desired,
        )
        RestoreTerminalPolicy.requireReceiptMatches(
            RestoreOperationState.ROLLED_BACK, desired, backup, challenge, "ROLLED_BACK", backup,
        )
        RestoreTerminalPolicy.requireReceiptMatches(
            RestoreOperationState.PREPARED, null, null, challenge, "ABORTED", challenge,
        )
        expectRejected {
            RestoreTerminalPolicy.requireReceiptMatches(
                RestoreOperationState.MUTATING, desired, backup, challenge, "COMMITTED", desired,
            )
        }
        expectRejected {
            RestoreTerminalPolicy.requireReceiptMatches(
                RestoreOperationState.COMMITTED, desired, backup, challenge, "COMMITTED", backup,
            )
        }
        expectRejected {
            RestoreTerminalPolicy.requireReceiptMatches(
                RestoreOperationState.COMMITTED, desired, null, challenge, "COMMITTED", desired,
            )
        }
        expectRejected {
            RestoreTerminalPolicy.requireReceiptMatches(
                RestoreOperationState.ROLLED_BACK, desired, backup, challenge, "ROLLED_BACK", desired,
            )
        }
    }

    @Test
    fun pendingRestoreCountTracksOnlyDurableLeaseFiles() {
        val root = Files.createTempDirectory("restore-pending-count").toFile()
        root.resolve("finished").mkdirs()
        root.resolve("pending-a").apply { mkdirs(); resolve("lease.json").writeText("fixture") }
        root.resolve("pending-b").apply { mkdirs(); resolve("lease.json").writeText("fixture") }

        assertEquals(2, RestoreRecovery.pendingCount(root))
        root.resolve("pending-a/lease.json").delete()
        assertEquals(1, RestoreRecovery.pendingCount(root))
        root.resolve("pending-b").deleteRecursively()
        assertEquals(0, RestoreRecovery.pendingCount(root))
        root.deleteRecursively()
    }

    @Test
    fun acquireFailureRemovesPlaceholderOperation() {
        val operation = Files.createTempDirectory("restore-acquire-failure").toFile()
        val placeholder = DurableRestoreLease(
            NemessixQuiescenceLease("", "a".repeat(64), "operation-a", ""),
            "tree-a",
        )

        runCatching {
            RestoreLeaseAcquireCoordinator.acquire(
                operation = operation,
                placeholder = placeholder,
                acquireLease = { error("nemessix_quiescence_unauthorized") },
            )
        }

        assertFalse(operation.exists())
    }

    @Test
    fun durableWriteFailureKeepsPlaceholderForRecovery() {
        val operation = Files.createTempDirectory("restore-acquire-durable-write").toFile()
        val placeholder = DurableRestoreLease(
            NemessixQuiescenceLease("", "b".repeat(64), "operation-b", ""),
            "tree-b",
        )
        var writes = 0

        runCatching {
            RestoreLeaseAcquireCoordinator.acquire(
                operation = operation,
                placeholder = placeholder,
                writeLease = { target, lease ->
                    writes += 1
                    if (writes == 2) error("durable write failed")
                    RestoreLeaseStore.write(target, lease)
                },
                acquireLease = {
                    NemessixQuiescenceLease("c".repeat(64), "b".repeat(64), "operation-b", "build")
                },
            )
        }

        assertTrue(operation.resolve("lease.json").isFile)
        assertEquals("", RestoreLeaseStore.read(operation).lease.leaseId)
        operation.deleteRecursively()
    }

    @Test
    fun ambiguousAcquireFailureKeepsPlaceholderForRecovery() {
        val operation = Files.createTempDirectory("restore-acquire-ambiguous").toFile()
        val placeholder = DurableRestoreLease(
            NemessixQuiescenceLease("", "d".repeat(64), "operation-d", ""),
            "tree-d",
        )

        runCatching {
            RestoreLeaseAcquireCoordinator.acquire(
                operation = operation,
                placeholder = placeholder,
                acquireLease = { error("nemessix_quiescence_challenge_mismatch") },
            )
        }

        assertTrue(operation.resolve("lease.json").isFile)
        operation.deleteRecursively()
    }

    @Test
    fun unavailableAcquireResponseKeepsPlaceholderForStatusRecovery() {
        val operation = Files.createTempDirectory("restore-acquire-unavailable").toFile()
        val placeholder = DurableRestoreLease(
            NemessixQuiescenceLease("", "e".repeat(64), "operation-e", ""),
            "tree-e",
        )

        runCatching {
            RestoreLeaseAcquireCoordinator.acquire(
                operation = operation,
                placeholder = placeholder,
                acquireLease = { error("nemessix_quiescence_unavailable") },
            )
        }

        assertTrue(operation.resolve("lease.json").isFile)
        operation.deleteRecursively()
    }

    private class SimulatedProcessDeath : Error()

    private fun expectRejected(block: () -> Unit) {
        try {
            block()
            fail("expected restore path rejection")
        } catch (_: IllegalStateException) {
            // expected
        }
    }
}
