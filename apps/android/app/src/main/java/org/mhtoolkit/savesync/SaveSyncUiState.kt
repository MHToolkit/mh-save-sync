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

enum class SaveSyncWorkflowStage(val persistedValue: String) {
    Idle("idle"),
    Check("check"),
    Confirm("confirm"),
    Write("write"),
    Complete("complete"),
    Blocked("blocked"),
    Unknown("unknown"),
    ;

    companion object {
        data class Transition(
            val phase: String,
            val error: String,
            val stage: SaveSyncWorkflowStage,
        )

        fun fromPersisted(value: String?): SaveSyncWorkflowStage = when (value) {
            null, "" -> Idle
            Idle.persistedValue -> Idle
            Check.persistedValue -> Check
            Confirm.persistedValue -> Confirm
            Write.persistedValue -> Write
            Complete.persistedValue -> Complete
            Blocked.persistedValue -> Blocked
            Unknown.persistedValue -> Unknown
            else -> Unknown
        }

        fun fromReason(reason: String?): SaveSyncWorkflowStage {
            val normalized = reason.orEmpty()
            return when {
                normalized.isBlank() -> Idle
                normalized.contains("failed") || normalized.contains("blocked") ||
                    normalized.contains("no-server") -> Blocked
                normalized.contains("confirm") || normalized.contains("conflict") -> Confirm
                normalized.contains("prelaunch") || normalized.contains("probe") -> Check
                normalized.contains("restore") || normalized.contains("upload") ||
                    normalized.contains("local") || normalized.contains("drain") ||
                    normalized.contains("periodic") || normalized.contains("manual-sync") ||
                    normalized.contains("download") || normalized.contains("session-exit") -> Write
                normalized == "session-start" -> Idle
                else -> Unknown
            }
        }

        fun fromPrelaunchState(state: PrelaunchConsistencyState): SaveSyncWorkflowStage = when (state) {
            PrelaunchConsistencyState.REMOTE_ADVANCED,
            PrelaunchConsistencyState.LOCAL_CHANGED,
            PrelaunchConsistencyState.DIVERGED,
            PrelaunchConsistencyState.UNKNOWN -> Confirm
            PrelaunchConsistencyState.NO_SERVER,
            PrelaunchConsistencyState.KEY_REQUIRED,
            PrelaunchConsistencyState.CLOUD_UNAVAILABLE,
            PrelaunchConsistencyState.LOCAL_UNAVAILABLE,
            PrelaunchConsistencyState.EMULATOR_RUNNING -> Blocked
            PrelaunchConsistencyState.SYNCED,
            PrelaunchConsistencyState.NO_REMOTE -> Confirm
        }

        fun prelaunchTransition(state: PrelaunchConsistencyState): Transition = Transition(
            phase = when (state) {
                PrelaunchConsistencyState.REMOTE_ADVANCED,
                PrelaunchConsistencyState.LOCAL_CHANGED,
                PrelaunchConsistencyState.DIVERGED,
                PrelaunchConsistencyState.UNKNOWN -> "等待确认"
                PrelaunchConsistencyState.NO_SERVER -> "需要设置服务器"
                PrelaunchConsistencyState.KEY_REQUIRED -> "需要恢复密钥"
                PrelaunchConsistencyState.CLOUD_UNAVAILABLE -> "云端暂不可用"
                PrelaunchConsistencyState.LOCAL_UNAVAILABLE -> "无法读取手机存档"
                PrelaunchConsistencyState.EMULATOR_RUNNING -> "Nemessix 正在运行"
                PrelaunchConsistencyState.SYNCED,
                PrelaunchConsistencyState.NO_REMOTE -> "检查完成"
            },
            error = when (state) {
                PrelaunchConsistencyState.NO_SERVER -> "prelaunch_no_server"
                PrelaunchConsistencyState.KEY_REQUIRED -> "prelaunch_key_required"
                PrelaunchConsistencyState.CLOUD_UNAVAILABLE -> "prelaunch_cloud_unavailable"
                PrelaunchConsistencyState.LOCAL_UNAVAILABLE -> "prelaunch_local_unavailable"
                PrelaunchConsistencyState.EMULATOR_RUNNING -> "prelaunch_emulator_running"
                else -> ""
            },
            stage = fromPrelaunchState(state),
        )

        fun legacyFallback(
            syncPhase: String,
            syncError: String,
        ): SaveSyncWorkflowStage {
            if (syncError.isNotBlank() || syncPhase.contains("失败") || syncPhase.contains("不可用")) {
                return Blocked
            }
            if (syncPhase == "检查完成" || syncPhase == "云端已检查") {
                return Confirm
            }
            if (syncPhase.contains("上传完成") || syncPhase.contains("恢复完成") ||
                syncPhase.contains("下载完成") || syncPhase.contains("同步完成") ||
                syncPhase.contains("后台对账完成") || syncPhase.contains("已处理") ||
                syncPhase.contains("已上传") || syncPhase.contains("已恢复")
            ) {
                return Complete
            }
            if (syncPhase.contains("等待确认") || syncPhase.contains("冲突")) {
                return Confirm
            }
            if (syncPhase.contains("检查")) {
                return Check
            }
            if (syncPhase.contains("上传") || syncPhase.contains("恢复") ||
                syncPhase.contains("下载") || syncPhase.contains("队列")
            ) {
                return Write
            }
            if (syncPhase.isBlank() || syncPhase == "暂无后台任务" || syncPhase == "游戏运行保护中") {
                return Idle
            }
            return Unknown
        }

        fun forTransition(
            reason: String?,
            syncPhase: String,
            syncError: String,
        ): SaveSyncWorkflowStage {
            val phaseStage = legacyFallback(
                syncPhase = syncPhase,
                syncError = syncError,
            )
            if (phaseStage == Unknown) return Unknown
            if (phaseStage != Idle && phaseStage != Unknown) return phaseStage
            val reasonStage = fromReason(reason)
            return if (reasonStage == Unknown) phaseStage else reasonStage
        }

        fun resolve(
            persistedValue: String?,
            reason: String?,
            syncPhase: String,
            syncError: String,
            uiPresentation: SaveSyncUiStatePresentation,
        ): SaveSyncWorkflowStage {
            val stored = fromPersisted(persistedValue)
            val reasonStage = fromReason(reason)
            val phaseStage = legacyFallback(
                syncPhase = syncPhase,
                syncError = syncError,
            )
            if (!persistedValue.isNullOrBlank()) {
                return stored
            }
            if (phaseStage != Unknown && phaseStage != Idle) return phaseStage
            if (reasonStage != Unknown && reasonStage != Idle) return reasonStage
            return phaseStage
        }
    }
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
            workflowStage: SaveSyncWorkflowStage = SaveSyncWorkflowStage.legacyFallback(
                syncPhase = syncPhase,
                syncError = syncError,
            ),
        ): SaveSyncStatusRailPresentation {
            val hasError = syncError.isNotBlank() ||
                uiPresentation.tone == SaveSyncUiTone.Error ||
                syncPhase.contains("失败") ||
                workflowStage == SaveSyncWorkflowStage.Blocked ||
                workflowStage == SaveSyncWorkflowStage.Unknown

            val checkTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                workflowStage == SaveSyncWorkflowStage.Complete -> SaveSyncStatusRailTone.Complete
                workflowStage in setOf(SaveSyncWorkflowStage.Confirm, SaveSyncWorkflowStage.Write) ->
                    SaveSyncStatusRailTone.Complete
                workflowStage == SaveSyncWorkflowStage.Check -> SaveSyncStatusRailTone.Current
                else -> SaveSyncStatusRailTone.Pending
            }
            val confirmTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                workflowStage == SaveSyncWorkflowStage.Complete -> SaveSyncStatusRailTone.Complete
                workflowStage == SaveSyncWorkflowStage.Write -> SaveSyncStatusRailTone.Complete
                workflowStage == SaveSyncWorkflowStage.Confirm && uiPresentation.isBlocking ->
                    SaveSyncStatusRailTone.Blocked
                workflowStage == SaveSyncWorkflowStage.Confirm -> SaveSyncStatusRailTone.Current
                workflowStage == SaveSyncWorkflowStage.Check -> SaveSyncStatusRailTone.Pending
                uiPresentation.isBlocking -> SaveSyncStatusRailTone.Blocked
                else -> SaveSyncStatusRailTone.Current
            }
            val writeTone = when {
                hasError -> SaveSyncStatusRailTone.Blocked
                workflowStage == SaveSyncWorkflowStage.Complete -> SaveSyncStatusRailTone.Complete
                workflowStage == SaveSyncWorkflowStage.Write -> SaveSyncStatusRailTone.Current
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
