package org.mhtoolkit.savesync

enum class SaveSyncUiTone {
    Success,
    Neutral,
    Warning,
    Error,
}

enum class SaveSyncStatusRailTone {
    Pending,
    Current,
    Complete,
    Blocked,
}

data class SaveSyncStatusRailStep(
    val label: String,
    val tone: SaveSyncStatusRailTone,
)

data class SaveSyncStatusRailPresentation(
    val steps: List<SaveSyncStatusRailStep>,
) {
    companion object {
        fun from(
            uiPresentation: SaveSyncUiStatePresentation,
            syncPhase: String,
            syncError: String,
        ): SaveSyncStatusRailPresentation {
            val terminalSuccessPhase = syncPhase.contains("完成") ||
                syncPhase.contains("已处理") ||
                syncPhase.contains("已上传") ||
                syncPhase.contains("已恢复")
            val hasError = syncError.isNotBlank() ||
                uiPresentation.tone == SaveSyncUiTone.Error ||
                syncPhase.contains("失败")
            val isRunning = syncPhase.contains("正在") ||
                syncPhase.contains("检查中") ||
                syncPhase.contains("上传中") ||
                syncPhase.contains("恢复中")
            val isComplete = uiPresentation.tone == SaveSyncUiTone.Success &&
                terminalSuccessPhase &&
                !uiPresentation.isBlocking &&
                !isRunning &&
                !hasError

            val checkTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                isComplete -> SaveSyncStatusRailTone.Complete
                isRunning -> SaveSyncStatusRailTone.Current
                else -> SaveSyncStatusRailTone.Pending
            }
            val confirmTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                isRunning -> SaveSyncStatusRailTone.Pending
                uiPresentation.isBlocking -> SaveSyncStatusRailTone.Blocked
                isComplete -> SaveSyncStatusRailTone.Complete
                else -> SaveSyncStatusRailTone.Current
            }
            val writeTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                isComplete -> SaveSyncStatusRailTone.Complete
                isRunning -> SaveSyncStatusRailTone.Current
                else -> SaveSyncStatusRailTone.Pending
            }
            return SaveSyncStatusRailPresentation(
                listOf(
                    SaveSyncStatusRailStep(SaveSyncDesignTokens.statusRailStepLabels[0], checkTone),
                    SaveSyncStatusRailStep(SaveSyncDesignTokens.statusRailStepLabels[1], confirmTone),
                    SaveSyncStatusRailStep(SaveSyncDesignTokens.statusRailStepLabels[2], writeTone),
                ),
            )
        }
    }
}

data class SaveSyncUiStatePresentation(
    val tone: SaveSyncUiTone,
    val status: String,
    val nextAction: String,
    val isBlocking: Boolean,
) {
    companion object {
        fun from(
            phase: String,
            error: String,
            pendingUploads: Int,
            conflictCount: Int,
            sessionActive: Boolean,
            authorized: Boolean,
            gameEnabled: Boolean,
            serverConfigured: Boolean,
        ): SaveSyncUiStatePresentation {
            if (error.isNotBlank() || phase.contains("失败")) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Error,
                    "同步未完成",
                    "检查网络、密钥和目录授权后重试；本地与云端均未被静默覆盖",
                    true,
                )
            }
            if (conflictCount > 0 || phase == "等待确认") {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Warning,
                    if (conflictCount > 0) "$conflictCount 个冲突待处理" else "等待确认",
                    "选择上传本地或恢复云端；不会自动覆盖",
                    true,
                )
            }
            if (pendingUploads > 0 || phase.contains("排队")) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Neutral,
                    if (pendingUploads > 0) "$pendingUploads 项等待网络恢复后续传" else "等待网络恢复后续传",
                    "队列会按原服务器地址继续，不会静默迁移或覆盖",
                    false,
                )
            }
            if (!gameEnabled) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Neutral,
                    "MH3G 同步已暂停",
                    "重新打开同步后仍会保留历史版本与显式冲突选择",
                    true,
                )
            }
            if (!authorized) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Warning,
                    "需要设置存档目录",
                    "选择 Android Nemessix 存档目录；授权不会立即上传或覆盖",
                    true,
                )
            }
            if (!serverConfigured) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Warning,
                    "需要设置服务器",
                    "填写和 Mac 相同的服务器地址；未填写前不会上传",
                    true,
                )
            }
            if (sessionActive) {
                return SaveSyncUiStatePresentation(
                    SaveSyncUiTone.Success,
                    "游玩中 · 本地已保护",
                    "退出 Nemessix 后再对账上传稳定快照",
                    false,
                )
            }
            return SaveSyncUiStatePresentation(
                SaveSyncUiTone.Success,
                "可以同步",
                "启动前检查云端；需要替换时会先备份并确认",
                false,
            )
        }
    }
}
