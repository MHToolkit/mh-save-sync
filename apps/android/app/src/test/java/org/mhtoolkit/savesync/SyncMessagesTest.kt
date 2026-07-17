package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
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

        assertTrue(message.contains("恢复云端版本"))
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
        assertTrue(message.contains("本机安全缓存"))
        assertTrue(message.contains("http://127.0.0.1:18080"))
        assertTrue(message.contains("端到端加密快照"))
    }

    @Test
    fun cloudActionWithoutServerExplainsWhyNothingUploaded() {
        val message = SyncMessages.cloudActionNeedsServer()

        assertTrue(message.contains("云端同步未开始"))
        assertTrue(message.contains("Mac 和 Android 必须填写同一个服务器地址"))
        assertTrue(message.contains("当前没有同步到任何服务器"))
    }

    @Test
    fun cloudUnavailableLaunchPauseRequiresExplicitLocalChoice() {
        val message = SyncMessages.launchPausedForCloudUnavailable()

        assertTrue(message.contains("已暂停自动打开 Nemessix"))
        assertTrue(message.contains("继续使用本地并打开 Nemessix"))
        assertTrue(message.contains("云端恢复后再补传"))
    }

    @Test
    fun manualActionMessagesExposeDirectionAndNoOverwriteSemantics() {
        val upload = SyncMessages.manualUploadQueued(
            target = "MH3G / Android Nemessix",
            serverEndpoint = "http://127.0.0.1:18080/",
        )
        val download = SyncMessages.downloadCacheQueued("http://127.0.0.1:18080/")
        val localLaunch = SyncMessages.continueLocalLaunchQueued()
        val replaceCloud = SyncMessages.localReplaceCloudQueued(
            target = "MH3G / Android Nemessix",
            serverEndpoint = "http://127.0.0.1:18080/",
            sessionActive = false,
        )

        assertTrue(upload.contains("同步到服务器"))
        assertTrue(upload.contains("MH3G / Android Nemessix → 本机安全缓存 → http://127.0.0.1:18080"))
        assertTrue(upload.contains("端到端加密上传"))
        assertTrue(download.contains("http://127.0.0.1:18080 → 本机安全缓存"))
        assertTrue(download.contains("不会覆盖 Nemessix 原目录"))
        assertTrue(localLaunch.contains("继续使用本地存档"))
        assertTrue(localLaunch.contains("不会从云端覆盖本地"))
        assertTrue(replaceCloud.contains("本地替换云端"))
        assertTrue(replaceCloud.contains("MH3G / Android Nemessix → 本机安全缓存 → http://127.0.0.1:18080"))
        assertTrue(replaceCloud.contains("云端旧版本会保留为历史/冲突分支"))
    }

    @Test
    fun localReplaceCloudExplainsConflictRetentionAndActiveSessionSafety() {
        val queuedWhilePlaying = SyncMessages.localReplaceCloudQueued(
            target = "MH3G / Android Nemessix",
            serverEndpoint = "http://127.0.0.1:18080/",
            sessionActive = true,
        )
        val processedWhilePlaying = SyncMessages.reconcileSummary(
            reason = "user-use-local",
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080/",
            sessionActive = true,
        )
        val processedStopped = SyncMessages.reconcileSummary(
            reason = "user-use-local",
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080/",
            sessionActive = false,
        )

        assertTrue(queuedWhilePlaying.contains("退出后上传"))
        assertTrue(queuedWhilePlaying.contains("不会上传正在写入的中间态"))
        assertTrue(queuedWhilePlaying.contains("云端旧版本会保留为历史/冲突分支"))
        assertTrue(processedWhilePlaying.contains("没有上传正在写入的中间态"))
        assertTrue(processedStopped.contains("已处理冲突选择"))
        assertTrue(processedStopped.contains("云端旧版本保留为历史/冲突分支"))

        listOf(queuedWhilePlaying, processedWhilePlaying, processedStopped).forEach { message ->
            assertTrue(!message.contains("锁定"))
            assertTrue(!message.contains("标记会话"))
            assertTrue(!message.contains("同步会话"))
            assertTrue(!message.contains("CAS"))
            assertTrue(!message.contains("HEAD"))
            assertTrue(!message.contains("SAF"))
        }
    }

    @Test
    fun activeSessionMessagesUsePlayerLanguageNotLockJargon() {
        val start = SyncMessages.sessionStartSummary()
        val exit = SyncMessages.sessionExitSummary()
        val blockedRestore = SyncMessages.restoreBlockedRunning()
        val buttonStart = SyncMessages.activeSessionToggleLabel(false)
        val buttonExit = SyncMessages.activeSessionToggleLabel(true)
        val channel = SyncMessages.activeSessionChannelName()
        val notificationTitle = SyncMessages.activeSessionNotificationTitle()
        val notificationText = SyncMessages.activeSessionNotificationText()

        assertTrue(start.contains("我正在玩 MH3G"))
        assertTrue(start.contains("本地存档保护"))
        assertTrue(exit.contains("我已退出 MH3G"))
        assertTrue(exit.contains("对账已排队"))
        assertTrue(blockedRestore.contains("我已退出 MH3G"))
        assertTrue(buttonStart == "我正在玩 MH3G（保护本地存档）")
        assertTrue(buttonExit == "我已退出 MH3G（开始对账上传）")
        assertTrue(channel.contains("游戏运行保护"))
        assertTrue(notificationTitle.contains("游戏运行保护"))
        assertTrue(notificationText.contains("正在玩 MH3G"))

        listOf(start, exit, blockedRestore, buttonStart, buttonExit, channel, notificationTitle, notificationText)
            .forEach { message ->
                assertTrue(!message.contains("锁定"))
                assertTrue(!message.contains("标记会话"))
                assertTrue(!message.contains("同步会话"))
            }
    }

    @Test
    fun legacyPersistedCopyIsSanitizedBeforeDisplay() {
        val fallback = "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。"

        assertTrue(
            SyncMessages.sanitizeLegacyUserCopy(
                value = "同步未执行：还没有授权 Android Nemessix 存档目录。请选择 SAF 目录后再试。",
                fallback = fallback,
            ) == fallback,
        )
        assertTrue(
            SyncMessages.sanitizeLegacyUserCopy(
                value = "已开启同步会话，请稍后查看。",
                fallback = fallback,
            ) == fallback,
        )
        assertTrue(
            SyncMessages.sanitizeLegacyUserCopy(
                value = "同步到服务器：MH3G / Android Nemessix → 本机安全缓存 → http://127.0.0.1:18080",
                fallback = fallback,
            ).contains("本机安全缓存"),
        )
        val neutral = "启动前会重新核对手机与云端版本。"
        assertEquals(
            neutral,
            SyncMessages.sanitizeLegacyUserCopy(
                value = "发现云端版本，请先选择上传或恢复。",
                fallback = neutral,
            ),
        )
        assertEquals(
            neutral,
            SyncMessages.sanitizeLegacyUserCopy(
                value = "云端有版本，请先确认同步方向",
                fallback = neutral,
            ),
        )
        val successful = "本地存档已设为云端最新（2 个文件）。"
        assertEquals(successful, SyncMessages.sanitizeLegacyUserCopy(successful, neutral))
    }

    @Test
    fun legacyPrelaunchReasonIsResetForFreshConsistencyCheck() {
        assertEquals("not-checked", SyncMessages.sanitizeLegacyPrelaunchReason("prelaunch-remote-head"))
        assertEquals("prelaunch-synced", SyncMessages.sanitizeLegacyPrelaunchReason("prelaunch-synced"))
        assertEquals("not-checked", SyncMessages.sanitizeLegacyPrelaunchReason(null))
    }

    @Test
    fun remoteVersionLabelDoesNotExposeBareSnapshotId() {
        val label = SyncServerProbe.userVisibleRemoteVersion(
            "243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2",
        )

        assertTrue(label.contains("云端已有一个版本"))
        assertTrue(label.contains("后 6 位"))
        assertTrue(label.contains("a41d2"))
        assertTrue(!label.contains("243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2"))
    }

    @Test
    fun prelaunchDecisionCopyExplainsImmediateActionsAndLocalRisk() {
        val decision = SyncMessages.prelaunchRemoteDecisionHint()
        val risk = SyncMessages.continueLocalRiskHint()

        assertTrue(decision.contains("只下载到本机缓存"))
        assertTrue(decision.contains("云端覆盖本地"))
        assertTrue(decision.contains("继续使用本地"))
        assertTrue(decision.contains("文件/字节级差异"))
        assertTrue(risk.contains("先不恢复云端"))
        assertTrue(risk.contains("冲突待处理"))
    }

    @Test
    fun conflictDiffBoundaryExplainsGameSpecificParserLimit() {
        val copy = SyncMessages.conflictDiffBoundary()

        assertTrue(copy.contains("MH3G/3U 3DS"))
        assertTrue(copy.contains("文件、大小、校验摘要"))
        assertTrue(copy.contains("变更字节段"))
        assertTrue(copy.contains("暂不声称能语义解析"))
        assertTrue(copy.contains("每个游戏会独立增加解析器"))
    }

    @Test
    fun backgroundStatusCopyExplainsQueueAndNextAction() {
        val phase = SyncMessages.queuedPhase("manual-upload")
        val next = SyncMessages.queuedNextAction("manual-upload", sessionActive = false)
        val blocked = SyncMessages.completedNextAction("restore-blocked-running", sessionActive = true)

        assertTrue(phase.contains("上传"))
        assertTrue(next.contains("等待存档稳定"))
        assertTrue(next.contains("加密上传到服务器"))
        assertTrue(blocked.contains("退出 MH3G"))
    }

    @Test
    fun noServerStatusCopyExplainsSharedServerBeforeActions() {
        val summary = SyncMessages.cloudActionNeedsServer()
        val phase = SyncMessages.noServerPhase()
        val next = SyncMessages.noServerNextAction("同步到服务器")
        val error = SyncMessages.noServerError()

        assertTrue(summary.contains("Mac 和 Android 必须填写同一个服务器地址"))
        assertTrue(phase.contains("服务器地址"))
        assertTrue(next.contains("Mac 和 Android 共用的服务器地址"))
        assertTrue(next.contains("同步到服务器"))
        assertTrue(error.contains("未配置服务器"))
    }

    @Test
    fun continueLocalStatusCopyExplainsLaterReconciliation() {
        val summary = SyncMessages.continueLocalLaunchQueued()
        val phase = SyncMessages.continueLocalPhase()
        val next = SyncMessages.continueLocalNextAction()

        assertTrue(summary.contains("继续使用本地存档"))
        assertTrue(phase.contains("继续使用本地"))
        assertTrue(next.contains("退出 MH3G 后"))
        assertTrue(next.contains("不会被静默覆盖"))
    }


    @Test
    fun dashboardSummaryExplainsCurrentStateAndNextAction() {
        val noServer = SyncMessages.dashboardStateSummary(
            authorized = true,
            gameEnabled = true,
            endpoint = "",
            sessionActive = false,
        )
        val noServerNext = SyncMessages.dashboardNextAction(
            authorized = true,
            gameEnabled = true,
            endpoint = "",
            sessionActive = false,
        )
        val ready = SyncMessages.dashboardStateSummary(
            authorized = true,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = false,
        )
        val playingNext = SyncMessages.dashboardNextAction(
            authorized = true,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = true,
        )

        val noAuthPrimary = SyncMessages.dashboardPrimaryActionLabel(
            authorized = false,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = false,
        )
        val noServerPrimary = SyncMessages.dashboardPrimaryActionLabel(
            authorized = true,
            gameEnabled = true,
            endpoint = "",
            sessionActive = false,
        )
        val readyPrimary = SyncMessages.dashboardPrimaryActionLabel(
            authorized = true,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = false,
        )
        val playingPrimary = SyncMessages.dashboardPrimaryActionLabel(
            authorized = true,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = true,
        )
        val readyHint = SyncMessages.dashboardPrimaryActionHint(
            authorized = true,
            gameEnabled = true,
            endpoint = "http://127.0.0.1:18080",
            sessionActive = false,
        )

        assertTrue(noServer.contains("还没有同步到服务器"))
        assertTrue(noServerNext.contains("未填写前不会上传到任何地方"))
        assertTrue(ready.contains("先做启动前检查"))
        assertTrue(playingNext.contains("我已退出 MH3G"))
        assertTrue(noAuthPrimary == "选择 Android Nemessix 存档目录")
        assertTrue(noServerPrimary == "到下方填写服务器地址")
        assertTrue(readyPrimary == "启动前检查")
        assertTrue(playingPrimary.contains("我已退出 MH3G"))
        assertTrue(readyHint.contains("检查不会修改本地存档"))
        listOf(noAuthPrimary, noServerPrimary, readyPrimary, playingPrimary, readyHint).forEach { message ->
            assertTrue(!message.contains("锁定"))
            assertTrue(!message.contains("标记会话"))
            assertTrue(!message.contains("同步会话"))
            assertTrue(!message.contains("SAF"))
            assertTrue(!message.contains("CAS"))
            assertTrue(!message.contains("HEAD"))
            assertTrue(!message.contains("dirty"))
            assertTrue(!message.contains("watcher"))
        }
    }

    @Test
    fun confirmationCopyExplainsRestoreAndLocalReplaceRisk() {
        val restore = SyncMessages.restoreCloudConfirmBody("http://127.0.0.1:18080/")
        val local = SyncMessages.localReplaceCloudConfirmBody(
            target = "MH3G / Android Nemessix",
            serverEndpoint = "http://127.0.0.1:18080/",
            sessionActive = true,
        )

        assertTrue(SyncMessages.restoreCloudConfirmTitle().contains("云端版本恢复本地"))
        assertTrue(restore.contains("先备份当前本地存档"))
        assertTrue(restore.contains("继续使用本地"))
        assertTrue(SyncMessages.localReplaceCloudConfirmTitle().contains("本地版本替换云端"))
        assertTrue(local.contains("不会立刻上传"))
        assertTrue(local.contains("云端旧版本会保留"))
        assertTrue(local.contains("不会按时间静默覆盖"))
    }


    @Test
    fun officeHomeFlowCopyExplainsSharedServerAndNoSilentOverwrite() {
        val steps = SyncMessages.officeHomeFlowSteps("http://127.0.0.1:18080/")
        val joined = steps.joinToString("\n")

        assertTrue(steps.size == 3)
        assertTrue(joined.contains("办公室 Mac"))
        assertTrue(joined.contains("回家 Android"))
        assertTrue(joined.contains("http://127.0.0.1:18080"))
        assertTrue(joined.contains("启动前检查"))
        assertTrue(joined.contains("退出 MH3G 后"))
        assertTrue(joined.contains("不会静默覆盖"))
    }

    @Test
    fun manualActionsIntroExplainsWhereSyncGoesBeforeButtonTap() {
        val configured = SyncMessages.manualActionsIntro(
            target = "MH3G / Android Nemessix",
            endpoint = "http://127.0.0.1:18080/",
        )
        val missing = SyncMessages.manualActionsIntro(
            target = "MH3G / Android Nemessix",
            endpoint = "",
        )

        assertTrue(configured.contains("MH3G / Android Nemessix → 本机安全缓存 → http://127.0.0.1:18080"))
        assertTrue(configured.contains("只下载云端到本机缓存"))
        assertTrue(configured.contains("云端覆盖本地前会二次确认"))
        assertTrue(configured.contains("本地替换云端会保留云端旧版本"))
        assertTrue(missing.contains("未配置服务器"))
        assertTrue(missing.contains("点同步也不会离开这台手机"))
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
