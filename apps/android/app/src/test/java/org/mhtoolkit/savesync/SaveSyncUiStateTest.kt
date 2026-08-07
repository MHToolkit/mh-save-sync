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
    fun `prelaunch check only marks check current without claiming a write`() {
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
        assertEquals(SaveSyncStatusRailTone.Pending, rail.steps[2].tone)
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

    @Test
    fun `typed workflow stage separates check write confirm complete blocked and idle`() {
        val neutral = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Neutral,
            status = "正在检查云端",
            nextAction = "等待",
            isBlocking = true,
        )
        assertEquals(
            listOf(SaveSyncStatusRailTone.Current, SaveSyncStatusRailTone.Pending, SaveSyncStatusRailTone.Pending),
            SaveSyncStatusRailPresentation.from(
                uiPresentation = neutral,
                syncPhase = "正在检查云端…",
                syncError = "",
                workflowStage = SaveSyncWorkflowStage.Check,
            ).steps.map { it.tone },
        )

        assertEquals(
            listOf(SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Current),
            SaveSyncStatusRailPresentation.from(
                uiPresentation = neutral.copy(status = "正在上传", isBlocking = false),
                syncPhase = "正在验证并上传",
                syncError = "",
                workflowStage = SaveSyncWorkflowStage.Write,
            ).steps.map { it.tone },
        )

        val conflict = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Warning,
            status = "等待确认",
            nextAction = "选择方向",
            isBlocking = true,
        )
        assertEquals(
            listOf(SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Pending),
            SaveSyncStatusRailPresentation.from(
                uiPresentation = conflict,
                syncPhase = "等待确认",
                syncError = "",
                workflowStage = SaveSyncWorkflowStage.Confirm,
            ).steps.map { it.tone },
        )

        val ready = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "检查云端",
            isBlocking = false,
        )
        assertEquals(
            listOf(SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Complete),
            SaveSyncStatusRailPresentation.from(ready, "上传完成", "", SaveSyncWorkflowStage.Complete)
                .steps.map { it.tone },
        )
        assertEquals(
            listOf(SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked),
            SaveSyncStatusRailPresentation.from(ready, "mystery", "", SaveSyncWorkflowStage.Unknown)
                .steps.map { it.tone },
        )
        assertEquals(
            listOf(SaveSyncStatusRailTone.Pending, SaveSyncStatusRailTone.Current, SaveSyncStatusRailTone.Pending),
            SaveSyncStatusRailPresentation.from(ready, "暂无后台任务", "", SaveSyncWorkflowStage.Idle)
                .steps.map { it.tone },
        )
    }

    @Test
    fun `workflow stage has stable reason mapping and legacy fallback`() {
        assertEquals(SaveSyncWorkflowStage.Check, SaveSyncWorkflowStage.fromReason("prelaunch-checking"))
        assertEquals(SaveSyncWorkflowStage.Write, SaveSyncWorkflowStage.fromReason("user-use-local"))
        assertEquals(SaveSyncWorkflowStage.Write, SaveSyncWorkflowStage.fromReason("restore-cloud-head"))
        assertEquals(SaveSyncWorkflowStage.Confirm, SaveSyncWorkflowStage.fromReason("user-use-local-confirm"))
        assertEquals(SaveSyncWorkflowStage.Blocked, SaveSyncWorkflowStage.fromReason("restore-blocked-running"))
        assertEquals(SaveSyncWorkflowStage.Unknown, SaveSyncWorkflowStage.fromPersisted("future-stage"))
        assertEquals(SaveSyncWorkflowStage.Check, SaveSyncWorkflowStage.legacyFallback("正在检查云端…", ""))
        assertEquals(SaveSyncWorkflowStage.Write, SaveSyncWorkflowStage.legacyFallback("正在安全恢复", ""))
        assertEquals(SaveSyncWorkflowStage.Confirm, SaveSyncWorkflowStage.legacyFallback("等待确认", ""))
        assertEquals(SaveSyncWorkflowStage.Blocked, SaveSyncWorkflowStage.legacyFallback("同步失败", "upload_failed"))
    }

    @Test
    fun `transition stage uses MainActivity reason phase and error combinations`() {
        assertEquals(
            SaveSyncWorkflowStage.Check,
            SaveSyncWorkflowStage.forTransition("prelaunch-checking", "正在检查云端…", ""),
        )
        assertEquals(
            SaveSyncWorkflowStage.Complete,
            SaveSyncWorkflowStage.forTransition("user-use-local", "上传完成", ""),
        )
        assertEquals(
            SaveSyncWorkflowStage.Complete,
            SaveSyncWorkflowStage.forTransition("restore-cloud-head", "恢复完成", ""),
        )
        assertEquals(
            SaveSyncWorkflowStage.Complete,
            SaveSyncWorkflowStage.forTransition("download-cache-only", "下载完成", ""),
        )
        assertEquals(
            SaveSyncWorkflowStage.Confirm,
            SaveSyncWorkflowStage.forTransition("user-use-local-confirm", "等待确认", ""),
        )
        assertEquals(
            SaveSyncWorkflowStage.Blocked,
            SaveSyncWorkflowStage.forTransition("user-use-local-failed", "上传失败", "同步失败"),
        )
    }

    @Test
    fun `missing persisted stage recovers constrained drain completion from phase`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "检查云端",
            isBlocking = false,
        )
        val resolved = SaveSyncWorkflowStage.resolve(
            persistedValue = null,
            reason = "constrained-drain",
            syncPhase = "上传完成",
            syncError = "",
            uiPresentation = ui,
        )
        assertEquals(SaveSyncWorkflowStage.Complete, resolved)
        assertEquals(
            listOf(SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Complete, SaveSyncStatusRailTone.Complete),
            SaveSyncStatusRailPresentation.from(
                uiPresentation = ui,
                syncPhase = "上传完成",
                syncError = "",
                workflowStage = resolved,
            ).steps.map { it.tone },
        )
    }

    @Test
    fun `nonempty persisted stage remains authoritative after writer audit`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Success,
            status = "可以同步",
            nextAction = "检查云端",
            isBlocking = false,
        )
        assertEquals(
            SaveSyncWorkflowStage.Write,
            SaveSyncWorkflowStage.resolve(
                persistedValue = SaveSyncWorkflowStage.Write.persistedValue,
                reason = "constrained-drain",
                syncPhase = "上传完成",
                syncError = "",
                uiPresentation = ui,
            ),
        )
    }

    @Test
    fun `unknown future phase fails closed instead of falling back to idle`() {
        val ui = SaveSyncUiStatePresentation(
            tone = SaveSyncUiTone.Neutral,
            status = "无法确认状态",
            nextAction = "保留本地与云端现状",
            isBlocking = false,
        )
        val stage = SaveSyncWorkflowStage.forTransition("future-reason", "未来状态", "")
        assertEquals(SaveSyncWorkflowStage.Unknown, stage)
        assertEquals(
            listOf(SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked),
            SaveSyncStatusRailPresentation.from(
                uiPresentation = ui,
                syncPhase = "未来状态",
                syncError = "",
                workflowStage = stage,
            ).steps.map { it.tone },
        )
    }

    @Test
    fun `prelaunch blocked transitions carry stable errors for presentation consistency`() {
        for (state in listOf(PrelaunchConsistencyState.KEY_REQUIRED, PrelaunchConsistencyState.CLOUD_UNAVAILABLE)) {
            val transition = SaveSyncWorkflowStage.prelaunchTransition(state)
            assertEquals(SaveSyncWorkflowStage.Blocked, transition.stage)
            assertTrue(transition.error.isNotBlank())

            val presentation = SaveSyncUiStatePresentation.from(
                phase = transition.phase,
                error = transition.error,
                pendingUploads = 0,
                conflictCount = 0,
                sessionActive = false,
                authorized = true,
                gameEnabled = true,
                serverConfigured = true,
            )
            assertEquals(SaveSyncUiTone.Error, presentation.tone)
            assertTrue(presentation.isBlocking)
            assertEquals(
                listOf(SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked, SaveSyncStatusRailTone.Blocked),
                SaveSyncStatusRailPresentation.from(
                    uiPresentation = presentation,
                    syncPhase = transition.phase,
                    syncError = transition.error,
                    workflowStage = transition.stage,
                ).steps.map { it.tone },
            )
        }
    }

    @Test
    fun `synced and no remote prelaunch checks stop at confirm without claiming write completion`() {
        for (state in listOf(PrelaunchConsistencyState.SYNCED, PrelaunchConsistencyState.NO_REMOTE)) {
            val transition = SaveSyncWorkflowStage.prelaunchTransition(state)
            assertEquals(SaveSyncWorkflowStage.Confirm, transition.stage)
            assertEquals("", transition.error)

            val presentation = SaveSyncUiStatePresentation.from(
                phase = transition.phase,
                error = transition.error,
                pendingUploads = 0,
                conflictCount = 0,
                sessionActive = false,
                authorized = true,
                gameEnabled = true,
                serverConfigured = true,
            )
            assertFalse(presentation.isBlocking)
            assertEquals(
                listOf(
                    SaveSyncStatusRailTone.Complete,
                    SaveSyncStatusRailTone.Current,
                    SaveSyncStatusRailTone.Pending,
                ),
                SaveSyncStatusRailPresentation.from(
                    uiPresentation = presentation,
                    syncPhase = transition.phase,
                    syncError = transition.error,
                    workflowStage = transition.stage,
                ).steps.map { it.tone },
            )
        }
    }
}
