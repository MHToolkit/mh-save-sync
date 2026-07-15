package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalReplacePolicyTest {
    @Test fun activeSessionAlwaysBlocksUpload() {
        assertThrows(IllegalStateException::class.java) { LocalReplacePolicy.requireSessionStopped(true) }
        LocalReplacePolicy.requireSessionStopped(false)
    }

    @Test fun confirmationMustBindTheObservedHead() {
        assertEquals("abc123", LocalReplacePolicy.requireObservedBase("abc123", "abc123"))
        assertThrows(IllegalStateException::class.java) {
            LocalReplacePolicy.requireObservedBase("abc123", "changed")
        }
    }

    @Test fun missingHeadIsRepresentedExplicitly() {
        assertEquals(null, LocalReplacePolicy.requireObservedBase(null, null))
        assertThrows(IllegalStateException::class.java) {
            LocalReplacePolicy.requireObservedBase(null, "created-by-other-device")
        }
    }

    @Test fun nativeResultNeverTreatsErrorAsSuccess() {
        val ok = LocalReplaceResult.parse(
            """{"outcome":"fast-forward","snapshot_id":"snapshot-a","cloud_head":"head-a","conflict_snapshot":null,"file_count":2,"total_bytes":47616}""",
        )
        assertTrue(ok is LocalReplaceResult.Uploaded)
        assertEquals("head-a", (ok as LocalReplaceResult.Uploaded).cloudHead)
        assertTrue(LocalReplaceResult.parse("""{"error":"sync_failed"}""") is LocalReplaceResult.Failed)
        assertTrue(LocalReplaceResult.parse("not-json") is LocalReplaceResult.Failed)
    }

    @Test fun conflictDoesNotClaimHeadWasReplaced() {
        val result = LocalReplaceResult.parse(
            """{"outcome":"conflict","snapshot_id":"branch-a","cloud_head":"head-old","conflict_snapshot":"branch-a","file_count":2,"total_bytes":12}""",
        )
        assertTrue(result is LocalReplaceResult.Conflict)
        assertFalse((result as LocalReplaceResult.Conflict).headChanged)
    }

    @Test fun onlySuccessfulUploadMayEstablishConsistencyLedger() {
        val uploaded = LocalReplaceResult.parse(
            """{"outcome":"fast-forward","snapshot_id":"snapshot-a","cloud_head":"head-a","file_count":2,"total_bytes":12}""",
        )
        val conflict = LocalReplaceResult.parse(
            """{"outcome":"conflict","snapshot_id":"branch-a","cloud_head":"head-old","conflict_snapshot":"branch-a"}""",
        )
        assertTrue(SyncLedgerWritePolicy.shouldEstablishAfterUpload(uploaded))
        assertFalse(SyncLedgerWritePolicy.shouldEstablishAfterUpload(conflict))
        assertFalse(SyncLedgerWritePolicy.shouldEstablishAfterUpload(LocalReplaceResult.Failed))
    }

    @Test fun `uploaded result needs cryptographic ids and confirmed current head before ledger write`() {
        val snapshot = "a".repeat(64)
        val valid = LocalReplaceResult.Uploaded(snapshot, snapshot, 2, 12)
        assertTrue(UploadConsistencyPolicy.canEstablish(valid, snapshot))
        assertFalse(UploadConsistencyPolicy.canEstablish(valid, "b".repeat(64)))
        assertFalse(UploadConsistencyPolicy.canEstablish(valid.copy(cloudHead = "b".repeat(64)), snapshot))
        assertFalse(UploadConsistencyPolicy.canEstablish(valid.copy(snapshotId = "not-hex"), snapshot))
        assertFalse(UploadConsistencyPolicy.canEstablish(valid.copy(cloudHead = "not-hex"), snapshot))
    }
}
