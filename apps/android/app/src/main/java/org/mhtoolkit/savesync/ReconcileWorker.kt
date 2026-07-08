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
                    "同步已 fail closed：Nemessix 存档目录权限被撤销，没有读取或覆盖任何本地存档。",
                )
                .apply()
            return Result.failure()
        }
        val endpoint = preferences.getString(SyncScheduler.SERVER_ENDPOINT, null)
        val target = preferences.getString(
            SyncScheduler.LAST_SYNC_TARGET,
            "MH3G / Android Nemessix",
        ) ?: "MH3G / Android Nemessix"
        val summary = SyncMessages.reconcileSummary(reason, target, endpoint)
        // The platform shell records user-visible state. Shared Rust performs the
        // stable-copy/hash/validate/encrypt/upload pipeline once JNI is loaded.
        preferences.edit()
            .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
            .putString(SyncScheduler.LAST_SYNC_REASON, reason)
            .putString(SyncScheduler.LAST_SYNC_SUMMARY, summary)
            .putString(SyncScheduler.LAST_SYNC_TARGET, target)
            .apply()
        return Result.success()
    }
}
