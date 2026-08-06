package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveSyncUiStateTest {
    @Test
    fun `conflict state is blocking and keeps an explicit next action`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "等待确认",
            error = "",
            pendingUploads = 0,
            conflictCount = 2,
            sessionActive = false,
            authorized = true,
            gameEnabled = true,
            serverConfigured = true,
        )

        assertEquals(SaveSyncUiTone.Warning, presentation.tone)
        assertTrue(presentation.isBlocking)
        assertEquals("选择上传本地或恢复云端；不会自动覆盖", presentation.nextAction)
    }

    @Test
    fun `offline queue explains retained work without claiming a write`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "上传排队中",
            error = "",
            pendingUploads = 3,
            conflictCount = 0,
            sessionActive = false,
            authorized = true,
            gameEnabled = true,
            serverConfigured = true,
        )

        assertEquals(SaveSyncUiTone.Neutral, presentation.tone)
        assertFalse(presentation.isBlocking)
        assertTrue(presentation.nextAction.contains("不会静默迁移或覆盖"))
    }

    @Test
    fun `missing directory remains blocking before server state`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "暂无后台任务",
            error = "",
            pendingUploads = 0,
            conflictCount = 0,
            sessionActive = false,
            authorized = false,
            gameEnabled = true,
            serverConfigured = false,
        )

        assertEquals("需要设置存档目录", presentation.status)
        assertTrue(presentation.isBlocking)
    }

    @Test
    fun `status rail does not mark default no task text as complete`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Warning,
            status = "需要设置存档目录",
            nextAction = "选择目录",
            isBlocking = true,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "暂无后台任务",
            syncError = "",
        )

        assertEquals(SaveSyncStatusRailTone.Pending, rail.steps[0].tone)
        assertEquals(SaveSyncStatusRailTone.Blocked, rail.steps[1].tone)
        assertEquals(SaveSyncStatusRailTone.Pending, rail.steps[2].tone)
    }

    @Test
    fun `status rail marks running work current without claiming completion`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Neutral,
            status = "正在检查云端",
            nextAction = "等待检查完成",
            isBlocking = true,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "正在检查云端…",
            syncError = "",
        )

        assertEquals(SaveSyncStatusRailTone.Current, rail.steps[0].tone)
        assertEquals(SaveSyncStatusRailTone.Pending, rail.steps[1].tone)
        assertEquals(SaveSyncStatusRailTone.Current, rail.steps[2].tone)
    }

    @Test
    fun `status rail blocks every step on error`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Error,
            status = "同步未完成",
            nextAction = "检查错误",
            isBlocking = true,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "恢复失败",
            syncError = "restore_failed",
        )

        assertEquals(
            listOf(
                SaveSyncStatusRailTone.Blocked,
                SaveSyncStatusRailTone.Blocked,
                SaveSyncStatusRailTone.Blocked,
            ),
            rail.steps.map { it.tone },
        )
    }

    @Test
    fun `status rail marks verified success complete`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "启动前检查云端",
            isBlocking = false,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "同步完成",
            syncError = "",
        )

        assertEquals(
            listOf(
                SaveSyncStatusRailTone.Complete,
                SaveSyncStatusRailTone.Complete,
                SaveSyncStatusRailTone.Complete,
            ),
            rail.steps.map { it.tone },
        )
    }

    @Test
    fun `configured fresh app is not complete without a terminal success phase`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "启动前检查云端",
            isBlocking = false,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "暂无后台任务",
            syncError = "",
        )

        assertEquals(
            listOf(
                SaveSyncStatusRailTone.Pending,
                SaveSyncStatusRailTone.Current,
                SaveSyncStatusRailTone.Pending,
            ),
            rail.steps.map { it.tone },
        )
    }

    @Test
    fun `session protection success tone is not a completed write`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "游玩中 · 本地已保护",
            nextAction = "退出 Nemessix 后再对账上传稳定快照",
            isBlocking = false,
        )

        val rail = SaveSyncStatusRailPresentation.from(
            uiPresentation = ui,
            syncPhase = "游戏运行保护中",
            syncError = "",
        )

        assertEquals(
            listOf(
                SaveSyncStatusRailTone.Pending,
                SaveSyncStatusRailTone.Current,
                SaveSyncStatusRailTone.Pending,
            ),
            rail.steps.map { it.tone },
        )
    }

    @Test
    fun `explicit upload and restore completion phases are complete`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "启动前检查云端",
            isBlocking = false,
        )

        for (phase in listOf("上传完成", "恢复完成")) {
            val rail = SaveSyncStatusRailPresentation.from(
                uiPresentation = ui,
                syncPhase = phase,
                syncError = "",
            )

            assertEquals(
                listOf(
                    SaveSyncStatusRailTone.Complete,
                    SaveSyncStatusRailTone.Complete,
                    SaveSyncStatusRailTone.Complete,
                ),
                rail.steps.map { it.tone },
            )
        }
    }
}
