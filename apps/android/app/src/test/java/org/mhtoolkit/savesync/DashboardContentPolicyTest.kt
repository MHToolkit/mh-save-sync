package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DashboardContentPolicyTest {
    @Test
    fun `default dashboard keeps only task-oriented sections visible`() {
        assertEquals(
            listOf("存档状态", "快速同步", "启动游戏", "最近记录"),
            DashboardContentPolicy.primarySections,
        )
        assertFalse(DashboardContentPolicy.primarySections.contains("端到端加密"))
        assertFalse(DashboardContentPolicy.primarySections.contains("Android Nemessix 存档目录"))
    }

    @Test
    fun `primary actions use short outcome-oriented Chinese labels`() {
        assertEquals("上传手机存档", DashboardContentPolicy.uploadLabel)
        assertEquals("恢复云端存档", DashboardContentPolicy.restoreLabel)
        assertEquals("检查并打开 Nemessix", DashboardContentPolicy.launchLabel)
        assertTrue(DashboardContentPolicy.uploadLabel.length <= 8)
        assertTrue(DashboardContentPolicy.restoreLabel.length <= 8)
    }

    @Test
    fun `long explanations belong to help instead of dashboard`() {
        assertEquals("设置", DashboardContentPolicy.settingsLabel)
        assertEquals("使用帮助", DashboardContentPolicy.helpLabel)
        assertTrue(DashboardContentPolicy.helpTopics.contains("冲突怎么处理"))
        assertTrue(DashboardContentPolicy.helpTopics.contains("同步安全吗"))
    }

    @Test
    fun `status card uses compact labels instead of paragraphs`() {
        assertEquals("需要设置存档目录", DashboardContentPolicy.status(false, true, true, false))
        assertEquals("需要设置服务器", DashboardContentPolicy.status(true, true, false, false))
        assertEquals("游玩中 · 本地已保护", DashboardContentPolicy.status(true, true, true, true))
        assertEquals("可以同步", DashboardContentPolicy.status(true, true, true, false))
        assertEquals("MH3G 同步已暂停", DashboardContentPolicy.status(true, false, true, false))
    }

    @Test
    fun `launch status never exposes stale head or server details`() {
        val remote = DashboardContentPolicy.launchStatus("prelaunch-unknown")
        assertEquals("首次检查，请选择使用手机或云端版本", remote)
        assertFalse(remote.contains("http"))
        assertFalse(remote.contains("版本摘要"))
        assertFalse(remote.contains("冲突"))
        assertEquals("手机与云端都有新进度，请选择方向", DashboardContentPolicy.launchStatus("prelaunch-diverged"))
        assertFalse(DashboardContentPolicy.launchStatus("prelaunch-diverged").contains("冲突"))
        assertEquals("启动前会先检查云端", DashboardContentPolicy.launchStatus("not-checked"))
    }

    @Test
    fun `restore failures select actionable user paths`() {
        assertEquals(
            RestoreFailureKind.EMULATOR_RUNNING,
            DashboardContentPolicy.restoreFailureKind(
                IllegalStateException("nemessix_quiescence_emulator_running"),
            ),
        )
        assertEquals(
            RestoreFailureKind.MH_SAVE_SYNC_NOT_TRUSTED,
            DashboardContentPolicy.restoreFailureKind(
                IllegalStateException("nemessix_quiescence_unauthorized"),
            ),
        )
        assertEquals(
            RestoreFailureKind.NEMESSIX_PROTOCOL_MISMATCH,
            DashboardContentPolicy.restoreFailureKind(
                IllegalStateException("nemessix_quiescence_protocol_mismatch"),
            ),
        )
        assertEquals(
            RestoreFailureKind.NEMESSIX_UNAVAILABLE,
            DashboardContentPolicy.restoreFailureKind(
                IllegalStateException("nemessix_quiescence_unavailable"),
            ),
        )
        assertEquals(
            RestoreFailureKind.NEMESSIX_UNAVAILABLE,
            DashboardContentPolicy.restoreFailureKind(
                IllegalStateException("nemessix_quiescence_untrusted_provider"),
            ),
        )
        assertEquals(
            RestoreFailureKind.OTHER,
            DashboardContentPolicy.restoreFailureKind(IllegalStateException("cloud_failed")),
        )
    }

    @Test
    fun `quiescence denial reasons have one protocol mapping`() {
        assertEquals(
            NemessixQuiescenceDenial.EMULATOR_RUNNING,
            NemessixQuiescenceDenial.fromProtocol("NOT_QUIESCENT"),
        )
        assertEquals(
            NemessixQuiescenceDenial.UNAUTHORIZED,
            NemessixQuiescenceDenial.fromProtocol("UNAUTHORIZED"),
        )
        assertEquals(
            NemessixQuiescenceDenial.UNKNOWN_OPERATION,
            NemessixQuiescenceDenial.fromProtocol("UNKNOWN_OPERATION"),
        )
        assertEquals(
            NemessixQuiescenceDenial.OTHER,
            NemessixQuiescenceDenial.fromProtocol("NEW_SERVER_REASON"),
        )
    }

    @Test
    fun `restore guidance is actionable and never promises an unproven rollback`() {
        val running = DashboardContentPolicy.restoreFailureGuidance(
            IllegalStateException("nemessix_quiescence_emulator_running"),
        )
        assertEquals("等待退出游戏", running.phase)
        assertTrue(running.action.contains("最近任务退出 Nemessix"))

        val unauthorized = DashboardContentPolicy.restoreFailureGuidance(
            IllegalStateException("nemessix_quiescence_unauthorized"),
        )
        assertEquals("需要更新 MH Save Sync", unauthorized.phase)
        assertTrue(unauthorized.summary.contains("签名"))
        assertTrue(unauthorized.action.contains("正式签名迁移版"))

        val protocolMismatch = DashboardContentPolicy.restoreFailureGuidance(
            IllegalStateException("nemessix_quiescence_protocol_mismatch"),
        )
        assertEquals("安全接口不兼容", protocolMismatch.phase)
        assertFalse(protocolMismatch.summary.contains("密钥"))

        val unavailable = DashboardContentPolicy.restoreFailureGuidance(
            IllegalStateException("nemessix_quiescence_unavailable"),
        )
        assertEquals("需要更新 Nemessix", unavailable.phase)

        val generic = DashboardContentPolicy.restoreFailureGuidance(
            IllegalStateException("restore_pending_recovery_failed"),
        )
        assertFalse(generic.summary.contains("已回滚"))
        assertFalse(generic.summary.contains("尝试回滚"))
        assertTrue(generic.action.contains("未完成的安全恢复"))
    }
}
