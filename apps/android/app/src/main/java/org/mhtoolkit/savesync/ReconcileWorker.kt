package org.mhtoolkit.savesync

import android.content.Context
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
        val root = preferences.getString(SyncScheduler.SAF_ROOT, null)
        val reason = inputData.getString("reason") ?: "periodic"
        if (root == null) {
            preferences.edit()
                .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    "同步未执行：还没有授权 Android Nemessix 存档目录。请选择存档目录后再试。",
                )
                .putString(SyncScheduler.LAST_SYNC_PHASE, "需要授权存档目录")
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "请点「选择 Android Nemessix 存档目录」后重试。")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "未授权存档目录")
                .apply()
            return Result.failure()
        }
        val stillGranted = applicationContext.contentResolver.persistedUriPermissions.any {
            it.uri.toString() == root && it.isReadPermission
        }
        if (!stillGranted) {
            preferences.edit()
                .remove(SyncScheduler.SAF_ROOT)
                .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    "同步已安全停止：Nemessix 存档目录权限被撤销，没有读取或覆盖任何本地存档。",
                )
                .putString(SyncScheduler.LAST_SYNC_PHASE, "目录权限已失效")
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "请重新选择 Android Nemessix 存档目录后再同步。")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "目录权限被撤销")
                .apply()
            return Result.failure()
        }
        val endpoint = preferences.getString(SyncScheduler.SERVER_ENDPOINT, null)
        val target = preferences.getString(
            SyncScheduler.LAST_SYNC_TARGET,
            "MH3G / Android Nemessix",
        ) ?: "MH3G / Android Nemessix"
        val sessionActive = preferences.getBoolean(SyncScheduler.SESSION_ACTIVE, true)
        if (!SyncScheduler.REAL_SYNC_PIPELINE_AVAILABLE) {
            // A periodic WorkManager request normally remains ENQUEUED between
            // its 15-minute runs. That is scheduler state, not a pending user
            // upload, so do not overwrite the dashboard with “排队中”.
            if (reason == "periodic") return Result.success()
            preferences.edit()
                .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    "当前 Android Alpha 尚未接入真实同步引擎；本次没有读取、上传、下载或覆盖任何存档。",
                )
                .putString(SyncScheduler.LAST_SYNC_PHASE, "未执行同步")
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "请等待带真实同步引擎的测试版本。")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "真实同步引擎尚未接入")
                .putString(SyncScheduler.LAST_SYNC_TARGET, target)
                .apply()
            return Result.success()
        }
        val summary = SyncMessages.reconcileSummary(
            reason = reason,
            target = target,
            endpoint = endpoint,
            sessionActive = sessionActive,
        )
        // The platform shell records user-visible state. Shared Rust performs the
        // stable-copy/hash/validate/encrypt/upload pipeline once JNI is loaded.
        preferences.edit()
            .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
            .putString(SyncScheduler.LAST_SYNC_REASON, reason)
            .putString(SyncScheduler.LAST_SYNC_SUMMARY, summary)
            .putString(SyncScheduler.LAST_SYNC_PHASE, SyncMessages.completedPhase(reason))
            .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, SyncMessages.completedNextAction(reason, sessionActive))
            .putString(SyncScheduler.LAST_SYNC_ERROR, "")
            .putString(SyncScheduler.LAST_SYNC_TARGET, target)
            .apply()
        return Result.success()
    }
}
