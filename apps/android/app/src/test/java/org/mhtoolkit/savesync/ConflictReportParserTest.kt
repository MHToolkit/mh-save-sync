package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ConflictReportParserTest {
    @Test
    fun parsesOnlyCompactNonPathSummary() {
        val report = ConflictReportParser.parse(
            """{"cloud_head":"${"a".repeat(64)}","conflicts":[{"snapshot_id":"${"b".repeat(64)}","cloud_head":"${"a".repeat(64)}","branch_device_id":"android-phone","branch_created_unix_ms":10,"cloud_device_id":"mac-home","cloud_created_unix_ms":20,"changed_files":2,"changed_bytes":47616}]}""",
        )
        assertEquals(1, report.branches.size)
        assertEquals(2, report.branches.single().changedFiles)
        assertEquals(47616, report.branches.single().changedBytes)
    }

    @Test
    fun emptyReportIsNotInventedAsConflict() {
        val report = ConflictReportParser.parse("""{"cloud_head":null,"conflicts":[]}""")
        assertNull(report.cloudHead)
        assertEquals(emptyList<ConflictBranchSummary>(), report.branches)
    }

    @Test(expected = IllegalStateException::class)
    fun nativeFailureFailsClosed() {
        ConflictReportParser.parse("""{"error":"conflict_fetch_failed"}""")
    }
}
