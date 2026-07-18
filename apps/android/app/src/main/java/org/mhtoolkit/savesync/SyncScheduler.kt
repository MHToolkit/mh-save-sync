package org.mhtoolkit.savesync

import android.content.Context
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

object SyncScheduler {
    const val REAL_SYNC_PIPELINE_AVAILABLE = true
    const val LOCAL_REPLACE_PIPELINE_AVAILABLE = true
    // Enabled only for Nemessix builds exposing the pinned SaveQuiescenceV1 lease.
    const val CLOUD_RESTORE_PIPELINE_AVAILABLE = true
    const val CLOUD_DOWNLOAD_PIPELINE_AVAILABLE = true
    const val PENDING_RESTORE_RECOVERY_COUNT = "pending_restore_recovery_count"
    const val PREFERENCES = "mh_save_sync"
    const val SAF_ROOT = "saf_root"
    const val WIFI_ONLY = "wifi_only"
    const val CHARGING_REQUIRED = "charging_required"
    const val DIRTY = "save_dirty"
    const val PENDING_UPLOAD_COUNT = "pending_upload_count"
    const val SERVER_ENDPOINT = "server_endpoint"
    const val LAST_SYNC_SUMMARY = "last_sync_summary"
    const val LAST_SYNC_TARGET = "last_sync_target"
    const val LAST_SYNC_REASON = "last_sync_reason"
    const val LAST_SYNC_PHASE = "last_sync_phase"
    const val LAST_SYNC_NEXT_ACTION = "last_sync_next_action"
    const val LAST_SYNC_ERROR = "last_sync_error"
    const val LAST_SYNC_UNIX_MS = "last_sync_unix_ms"
    const val REMOTE_VERSION_LABEL = "remote_version_label"
    const val LAUNCH_GATE_SUMMARY = "launch_gate_summary"
    const val LAUNCH_GATE_REASON = "launch_gate_reason"
    const val SESSION_ACTIVE = "session_active"
    const val GAME_MH3G_ENABLED = "game_mh3g_enabled"
    const val NATIVE_BRIDGE_HEALTH = "native_bridge_health"
    const val NEMESSIX_PACKAGE = "io.github.vincentadamnemessisx.nemessix"
    private const val PERIODIC_NAME = "save-reconcile-periodic"
    private const val IMMEDIATE_NAME = "save-reconcile-immediate"

    fun ensureDefaults(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val defaultLastSyncSummary =
            "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。"
        val defaultLaunchGateSummary =
            "启动前会重新核对手机与云端版本。"
        val defaultPhase = "暂无后台任务"
        val defaultNextAction = "先完成设置，再点“检查并打开 Nemessix”。"
        if (!preferences.contains(LAST_SYNC_TARGET)) {
            preferences.edit()
                .putBoolean(GAME_MH3G_ENABLED, true)
                .putBoolean(SESSION_ACTIVE, true)
                .putString(LAST_SYNC_TARGET, "MH3G / Android Nemessix")
                .putString(LAST_SYNC_SUMMARY, defaultLastSyncSummary)
                .putString(LAST_SYNC_PHASE, defaultPhase)
                .putString(LAST_SYNC_NEXT_ACTION, defaultNextAction)
                .putString(LAST_SYNC_ERROR, "")
                .putString(LAUNCH_GATE_SUMMARY, defaultLaunchGateSummary)
                .putString(LAUNCH_GATE_REASON, "not-checked")
                .apply()
        }
        val oldLastReason = preferences.getString(LAST_SYNC_REASON, null)
        val oldLaunchReason = preferences.getString(LAUNCH_GATE_REASON, null)
        val cleanLastReason = SyncMessages.sanitizeLegacyPrelaunchReason(oldLastReason)
        val cleanLaunchReason = SyncMessages.sanitizeLegacyPrelaunchReason(oldLaunchReason)
        val cleanLastSyncSummary = if (oldLastReason == "prelaunch-remote-head") {
            defaultLastSyncSummary
        } else {
            SyncMessages.sanitizeLegacyUserCopy(
                preferences.getString(LAST_SYNC_SUMMARY, null),
                defaultLastSyncSummary,
            )
        }
        val cleanLaunchGateSummary = if (oldLaunchReason == "prelaunch-remote-head") {
            defaultLaunchGateSummary
        } else {
            SyncMessages.sanitizeLegacyUserCopy(
                preferences.getString(LAUNCH_GATE_SUMMARY, null),
                defaultLaunchGateSummary,
            )
        }
        val cleanNextAction = SyncMessages.sanitizeLegacyUserCopy(
            preferences.getString(LAST_SYNC_NEXT_ACTION, null),
            "Mac 端同步后即可看到该版本。",
        )
        if (
            cleanLastSyncSummary != preferences.getString(LAST_SYNC_SUMMARY, null) ||
            cleanLaunchGateSummary != preferences.getString(LAUNCH_GATE_SUMMARY, null) ||
            cleanNextAction != preferences.getString(LAST_SYNC_NEXT_ACTION, null) ||
            cleanLastReason != oldLastReason || cleanLaunchReason != oldLaunchReason
        ) {
            preferences.edit()
                .putString(LAST_SYNC_SUMMARY, cleanLastSyncSummary)
                .putString(LAUNCH_GATE_SUMMARY, cleanLaunchGateSummary)
                .putString(LAST_SYNC_NEXT_ACTION, cleanNextAction)
                .putString(LAST_SYNC_REASON, cleanLastReason)
                .putString(LAUNCH_GATE_REASON, cleanLaunchReason)
                .apply()
        }
    }

    fun ensurePeriodic(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val network = if (preferences.getBoolean(WIFI_ONLY, true)) {
            NetworkType.UNMETERED
        } else {
            NetworkType.CONNECTED
        }
        val constraints = Constraints.Builder()
            .setRequiredNetworkType(network)
            .setRequiresBatteryNotLow(true)
            .setRequiresCharging(preferences.getBoolean(CHARGING_REQUIRED, false))
            .build()
        val request = PeriodicWorkRequestBuilder<ReconcileWorker>(15, TimeUnit.MINUTES)
            .setConstraints(constraints)
            .setInputData(Data.Builder().putString("reason", "periodic").build())
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(
            PERIODIC_NAME,
            ExistingPeriodicWorkPolicy.UPDATE,
            request,
        )
    }

    fun enqueueImmediate(context: Context, reason: String) {
        val request = OneTimeWorkRequestBuilder<ReconcileWorker>()
            .setInputData(Data.Builder().putString("reason", reason).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            IMMEDIATE_NAME,
            ExistingWorkPolicy.APPEND_OR_REPLACE,
            request,
        )
    }

    fun markDirty(context: Context) {
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            .edit().putBoolean(DIRTY, true).apply()
    }
}
