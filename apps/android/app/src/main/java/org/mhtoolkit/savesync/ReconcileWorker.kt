package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters

class ReconcileWorker(
    appContext: Context,
    params: WorkerParameters,
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        val preferences = applicationContext.getSharedPreferences(
            SyncScheduler.PREFERENCES,
            Context.MODE_PRIVATE,
        )
        val reason = inputData.getString("reason") ?: "periodic"
        val endpoint = preferences.getString(SyncScheduler.SERVER_ENDPOINT, null).orEmpty()
        val target = preferences.getString(
            SyncScheduler.LAST_SYNC_TARGET,
            "MH3G / Android Nemessix",
        ) ?: "MH3G / Android Nemessix"
        if (!preferences.getBoolean(SyncScheduler.GAME_MH3G_ENABLED, true)) {
            return Result.success()
        }

        val root = preferences.getString(SyncScheduler.SAF_ROOT, null)
        val stillGranted = root != null && applicationContext.contentResolver.persistedUriPermissions.any {
            it.uri.toString() == root && it.isReadPermission
        }
        if (root != null && !stillGranted) {
            preferences.edit().remove(SyncScheduler.SAF_ROOT).apply()
        }
        val dirty = preferences.getBoolean(SyncScheduler.DIRTY, false)
        val sessionActive = preferences.getBoolean(SyncScheduler.SESSION_ACTIVE, true)
        val result = DurableSyncPipeline(applicationContext).execute(
            reason = reason,
            treeUri = root?.takeIf { stillGranted }?.let(Uri::parse),
            serverEndpoint = endpoint,
            dirty = dirty,
            sessionActive = sessionActive,
        )
        if (result.queued) {
            preferences.edit().putBoolean(SyncScheduler.DIRTY, false).apply()
        }
        val phase: String
        val summary: String
        val nextAction: String
        val error: String
        when {
            result.localError == "server_required" -> {
                phase = "需要服务器地址"
                summary = "同步未执行：请先填写云存档服务器地址。"
                nextAction = "填写服务器地址后重试；本地原始存档没有变化。"
                error = "未配置服务器"
            }
            result.localError == "recovery_secret_required" -> {
                phase = "需要恢复密钥"
                summary = "同步未执行：请先导入与其他设备相同的恢复密钥。"
                nextAction = "导入恢复密钥后重试；密钥不会上传到服务器。"
                error = "未导入恢复密钥"
            }
            result.localError == "saf_permission_required" -> {
                phase = "需要授权存档目录"
                summary = "已有加密上传任务会继续重试，但无法读取新的 Android 存档候选。"
                nextAction = "请重新选择 Android Nemessix 存档目录。"
                error = "存档目录权限不可用"
            }
            result.localError != null -> {
                phase = "本地快照未创建"
                summary = "同步安全停止：没有上传不稳定存档，也没有修改本地原始存档。"
                nextAction = "确认 Nemessix 已退出、目录权限有效后重试。"
                error = "稳定快照或持久队列创建失败"
            }
            result.conflictCount > 0 -> {
                phase = "检测到冲突"
                summary = "手机快照已安全上传为冲突分支，没有静默覆盖云端或本地存档。"
                nextAction = "打开冲突页面，比较两边版本后明确选择。"
                error = ""
            }
            result.pendingCount > 0 -> {
                phase = "离线队列待续传"
                summary = "稳定快照已端到端加密并保存在手机队列中；云端不可用时不会丢失任务。"
                nextAction = "保持网络可用，后台任务会自动续传。"
                error = "云端暂时不可用"
            }
            result.uploadedCount > 0 -> {
                phase = "上传完成"
                summary = "Android 稳定快照已加密上传到云存档服务器。"
                nextAction = "可以在 Mac 端检查并恢复这个版本。"
                error = ""
            }
            else -> {
                phase = if (reason == "periodic") "定时对账完成" else "无需上传"
                summary = "没有待续传任务，也没有需要创建的新稳定快照。"
                nextAction = "退出游戏或收到保存完成事件后会再次检查。"
                error = ""
            }
        }
        preferences.edit()
            .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
            .putString(SyncScheduler.LAST_SYNC_REASON, reason)
            .putString(SyncScheduler.LAST_SYNC_SUMMARY, summary)
            .putString(SyncScheduler.LAST_SYNC_PHASE, phase)
            .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, nextAction)
            .putString(SyncScheduler.LAST_SYNC_ERROR, error)
            .putString(SyncScheduler.LAST_SYNC_TARGET, target)
            .putInt(SyncScheduler.PENDING_UPLOAD_COUNT, result.pendingCount)
            .apply()
        return if (result.shouldRetry) Result.retry() else Result.success()
    }
}
