package org.mhtoolkit.savesync

enum class SaveSyncUiTone {
    Success,
    Neutral,
    Warning,
    Error,
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
            authorized: Boolean = true,
            gameEnabled: Boolean = true,
            serverConfigured: Boolean = true,
        ): SaveSyncUiStatePresentation {
            if (error.isNotBlank() || phase.contains("失败")) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Error,
                    status = "同步未完成",
                    nextAction = "检查网络、密钥和目录授权后重试；本地与云端均未被静默覆盖",
                    isBlocking = true,
                )
            }
            if (conflictCount > 0 || phase == "等待确认") {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Warning,
                    status = if (conflictCount > 0) "$conflictCount 个冲突待处理" else "等待确认",
                    nextAction = "选择上传本地或恢复云端；不会自动覆盖",
                    isBlocking = true,
                )
            }
            if (pendingUploads > 0 || phase.contains("排队")) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Neutral,
                    status = "$pendingUploads 项等待网络恢复后续传",
                    nextAction = "保持网络可用；队列会按原服务器地址继续，不会静默覆盖",
                    isBlocking = false,
                )
            }
            if (!gameEnabled) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Neutral,
                    status = "MH3G 同步已暂停",
                    nextAction = "重新打开同步后仍会保留历史版本与显式冲突选择",
                    isBlocking = true,
                )
            }
            if (!authorized) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Warning,
                    status = "需要设置存档目录",
                    nextAction = "选择 Android Nemessix 存档目录；授权不会立即上传或覆盖",
                    isBlocking = true,
                )
            }
            if (!serverConfigured) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Warning,
                    status = "需要设置服务器",
                    nextAction = "填写和 Mac 相同的服务器地址；未填写前不会上传",
                    isBlocking = true,
                )
            }
            if (sessionActive) {
                return SaveSyncUiStatePresentation(
                    tone = SaveSyncUiTone.Success,
                    status = "游玩中 · 本地已保护",
                    nextAction = "退出 Nemessix 后再对账上传稳定快照",
                    isBlocking = false,
                )
            }
            return SaveSyncUiStatePresentation(
                tone = SaveSyncUiTone.Success,
                status = "可以同步",
                nextAction = "启动前检查云端；需要替换时会先备份并确认",
                isBlocking = false,
            )
        }
    }
}
