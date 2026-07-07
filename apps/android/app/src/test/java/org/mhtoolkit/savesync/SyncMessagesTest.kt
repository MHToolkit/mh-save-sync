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

    @Test
    fun syncRouteExplainsServerAndLocalCas() {
        val message = SyncMessages.syncRoute(
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080/",
        )

        assertTrue(message.contains("MH3G / Android Nemessix"))
        assertTrue(message.contains("staging/CAS"))
        assertTrue(message.contains("http://127.0.0.1:18080/"))
        assertTrue(message.contains("端到端加密快照"))
    }

    @Test
    fun cloudActionWithoutServerExplainsWhyNothingUploaded() {
        val message = SyncMessages.cloudActionNeedsServer()

        assertTrue(message.contains("云端同步未开始"))
        assertTrue(message.contains("Mac 和 Android 必须填写同一个服务器地址"))
    }

    @Test
    fun cloudUnavailableLaunchPauseRequiresExplicitLocalChoice() {
        val message = SyncMessages.launchPausedForCloudUnavailable()

        assertTrue(message.contains("已暂停自动打开 Nemessix"))
        assertTrue(message.contains("手动打开 Nemessix 继续使用本地存档"))
        assertTrue(message.contains("云端恢复后再补传"))
    }

    @Test
    fun prelaunchProbeUsesStableMh3gLogicalSaveIdAndNormalizesServer() {
        assertTrue(
            SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID
                .matches(Regex("[0-9a-f]{64}")),
        )
        assertTrue(
            SyncServerProbe.normalizeServer(" http://127.0.0.1:18080/// ") ==
                "http://127.0.0.1:18080",
        )
    }
}
