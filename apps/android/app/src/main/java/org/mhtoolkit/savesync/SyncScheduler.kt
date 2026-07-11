package org.mhtoolkit.savesync

import android.content.Context
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

object SyncScheduler {
    // Flip only after a real SAF -> stable snapshot -> E2EE -> server pipeline is
    // loaded and covered by device tests. The current Android Alpha is a UI shell.
    const val REAL_SYNC_PIPELINE_AVAILABLE = false
    const val LOCAL_REPLACE_PIPELINE_AVAILABLE = true
    const val PREFERENCES = "mh_save_sync"
    const val SAF_ROOT = "saf_root"
    const val WIFI_ONLY = "wifi_only"
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

    fun ensureDefaults(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val defaultLastSyncSummary =
            "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。"
        val defaultLaunchGateSummary =
            "未检查。启动 MH3G 前点「启动前检查」。"
        val defaultPhase = "暂无后台任务"
        val defaultNextAction = "先填写服务器地址并授权存档目录，然后做启动前检查。"
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
        val cleanLastSyncSummary = SyncMessages.sanitizeLegacyUserCopy(
            preferences.getString(LAST_SYNC_SUMMARY, null),
            defaultLastSyncSummary,
        )
        val cleanLaunchGateSummary = SyncMessages.sanitizeLegacyUserCopy(
            preferences.getString(LAUNCH_GATE_SUMMARY, null),
            defaultLaunchGateSummary,
        )
        if (
            cleanLastSyncSummary != preferences.getString(LAST_SYNC_SUMMARY, null) ||
            cleanLaunchGateSummary != preferences.getString(LAUNCH_GATE_SUMMARY, null)
        ) {
            preferences.edit()
                .putString(LAST_SYNC_SUMMARY, cleanLastSyncSummary)
                .putString(LAUNCH_GATE_SUMMARY, cleanLaunchGateSummary)
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
        WorkManager.getInstance(context).enqueue(request)
    }
}
