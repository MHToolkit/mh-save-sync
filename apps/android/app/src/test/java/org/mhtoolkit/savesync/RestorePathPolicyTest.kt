package org.mhtoolkit.savesync

import java.nio.file.Files
import kotlin.io.path.writeText
import org.junit.Assert.assertEquals
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
    fun processDeathLeavesDurableStateAndRetryRollsBack() {
        val operation = Files.createTempDirectory("restore-operation").toFile()
        val desired = operation.resolve("incoming").apply { mkdirs(); resolve("save").writeText("new") }
        val backup = operation.resolve("before").apply { mkdirs(); resolve("save").writeText("old") }
        var live = "old"
        val dyingBackend = RestoreTreeBackend { source ->
            live = source.resolve("save").readText()
            throw SimulatedProcessDeath()
        }
        try {
            RestoreTransaction(DurableRestoreState(operation), dyingBackend).commit(desired, backup)
            fail("expected simulated process death")
        } catch (_: SimulatedProcessDeath) { }
        assertEquals(RestoreOperationState.MUTATING, DurableRestoreState(operation).read())
        assertEquals("new", live)
        RestoreTransaction(DurableRestoreState(operation), RestoreTreeBackend { source ->
            live = source.resolve("save").readText()
        }).recover(backup)
        assertEquals("old", live)
        assertEquals(RestoreOperationState.ROLLED_BACK, DurableRestoreState(operation).read())
        operation.deleteRecursively()
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
    fun stoppedGateRequiresVerifiedIpcLease() {
        val now = 5_000L
        expectRejected { RestoreStopGate.requireFreshConfirmation(RestoreStopEvidence(now, true, false), now) }
        RestoreStopGate.requireFreshConfirmation(RestoreStopEvidence(now, true, true), now)
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
