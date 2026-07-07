package org.mhtoolkit.savesync

import org.junit.Assert.assertTrue
import org.junit.Test

class SyncMessagesTest {
    @Test
    fun restoreCloudHeadMessageExplainsStoppedPreconditionAndBackup() {
        val message = SyncMessages.reconcileSummary(
            reason = "restore-cloud-head",
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080",
        )

        assertTrue(message.contains("恢复云端 HEAD"))
        assertTrue(message.contains("Nemessix 已停止"))
        assertTrue(message.contains("先备份当前本地存档"))
        assertTrue(message.contains("http://127.0.0.1:18080"))
    }

    @Test
    fun runningRestoreMessageFailsClosedWithoutOverwrite() {
        val message = SyncMessages.reconcileSummary(
            reason = "restore-blocked-running",
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080",
        )

        assertTrue(message.contains("已拒绝恢复"))
        assertTrue(message.contains("Nemessix 仍在运行"))
        assertTrue(message.contains("没有覆盖本地存档"))
    }
}
