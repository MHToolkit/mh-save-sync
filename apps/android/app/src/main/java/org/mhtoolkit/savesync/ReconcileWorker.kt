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
                    "同步未执行：还没有授权 Android Nemessix 存档目录。请选择 SAF 目录后再试。",
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
        )
        val summary = when (reason) {
            "manual-upload" -> "已处理手动上传：$target 会经过稳定窗口、staging 复制、manifest/hash 校验后，上传到 ${endpoint.orEmpty().ifBlank { "未配置服务器" }}。"
            "session-exit" -> "已处理退出后对账：Nemessix 停止后才允许恢复；若有稳定本地快照，会加密排队上传。"
            "user-use-local" -> "已处理冲突选择：本地版本作为新的当前快照上传；云端旧版本保留为历史/冲突分支。"
            "download-cache-only" -> "已处理只下载：云端快照只进入本地缓存，不会覆盖正在运行的 Nemessix 存档目录。"
            "periodic" -> "已执行 15 分钟级兜底对账：无变化不会全量读取；发现 dirty 也必须等稳定快照后再上传。"
            else -> "已执行对账：原因=$reason；同步目标=$target；不会静默覆盖本地或云端。"
        }
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
