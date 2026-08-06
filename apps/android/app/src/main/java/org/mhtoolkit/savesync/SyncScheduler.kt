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
import java.util.concurrent.atomic.AtomicInteger

object SyncScheduler {
    const val REAL_SYNC_PIPELINE_AVAILABLE = true
    const val SAVE_COMPLETE_IPC_AVAILABLE = false
    // ActivityManager cross-package visibility is not yet proven on the target device/API.
    const val PROCESS_EXIT_RUNTIME_VERIFIED = false
    const val LOCAL_REPLACE_PIPELINE_AVAILABLE = true
    // Restore uses the certificate-pinned SaveQuiescenceV1 lease; it is not save-complete IPC.
    const val CLOUD_RESTORE_PIPELINE_AVAILABLE = true
    const val CLOUD_DOWNLOAD_PIPELINE_AVAILABLE = true
    const val PENDING_RESTORE_RECOVERY_COUNT = "pending_restore_recovery_count"
    const val PREFERENCES = "mh_save_sync"
    const val SAF_ROOT = "saf_root"
    const val WIFI_ONLY = "wifi_only"
    const val CHARGING_REQUIRED = "charging_required"
    private const val LEGACY_DIRTY = "save_dirty"
    private const val SESSION_TRACKING_VERSION = "session_tracking_version"
    internal const val LEGACY_PERIODIC_NAME = "save-reconcile-periodic"
    internal const val LEGACY_IMMEDIATE_NAME = "save-reconcile-immediate"
    internal const val LEGACY_WORK_MIGRATION_VERSION = "legacy_work_migration_version"
    const val PENDING_UPLOAD_COUNT = "pending_upload_count"
    const val PENDING_UPLOAD_ENDPOINT_COUNT = "pending_upload_endpoint_count"
    const val SERVER_ENDPOINT = "server_endpoint"
    const val LAST_SYNC_SUMMARY = "last_sync_summary"
    const val LAST_SYNC_TARGET = "last_sync_target"
    const val LAST_SYNC_REASON = "last_sync_reason"
    const val LAST_SYNC_PHASE = "last_sync_phase"
    const val LAST_SYNC_WORKFLOW_STAGE = "last_sync_workflow_stage"
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
    private const val PERIODIC_NAME = "save-capture-periodic"
    private const val CAPTURE_NAME = "save-capture-immediate"
    private const val DRAIN_NAME = "save-upload-drain"

    internal fun captureConstraints(chargingRequired: Boolean): Constraints =
        Constraints.Builder()
            .setRequiresBatteryNotLow(true)
            .setRequiresCharging(chargingRequired)
            .build()

    internal fun drainConstraints(wifiOnly: Boolean, chargingRequired: Boolean): Constraints =
        Constraints.Builder()
            .setRequiredNetworkType(if (wifiOnly) NetworkType.UNMETERED else NetworkType.CONNECTED)
            .setRequiresBatteryNotLow(true)
            .setRequiresCharging(chargingRequired)
            .build()

    fun ensureDefaults(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val defaultLastSyncSummary =
            "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。"
        val defaultLaunchGateSummary = "启动前会重新核对手机与云端版本。"
        val defaultPhase = "暂无后台任务"
        val defaultNextAction = "先完成设置，再点“检查并打开 Nemessix”。"
        migrateLegacyWorkManager(context)
        if (!preferences.contains(LAST_SYNC_TARGET)) {
            preferences.edit()
                .putBoolean(GAME_MH3G_ENABLED, true)
                .putBoolean(SESSION_ACTIVE, false)
                .putString(LAST_SYNC_TARGET, "MH3G / Android Nemessix")
                .putString(LAST_SYNC_SUMMARY, defaultLastSyncSummary)
                .putString(LAST_SYNC_PHASE, defaultPhase)
                .putString(LAST_SYNC_WORKFLOW_STAGE, SaveSyncWorkflowStage.Idle.persistedValue)
                .putString(LAST_SYNC_NEXT_ACTION, defaultNextAction)
                .putString(LAST_SYNC_ERROR, "")
                .putString(LAUNCH_GATE_SUMMARY, defaultLaunchGateSummary)
                .putString(LAUNCH_GATE_REASON, "not-checked")
                .apply()
        }
        if (preferences.getInt(SESSION_TRACKING_VERSION, 0) < 1) {
            val editor = preferences.edit()
                .putInt(SESSION_TRACKING_VERSION, 1)
                // Alpha.3 defaulted this flag to true without real process tracking.
                .putBoolean(SESSION_ACTIVE, false)
            if (preferences.getBoolean(LEGACY_DIRTY, false)) {
                editor.remove(LEGACY_DIRTY)
                markDirty(context)
            }
            editor.commit()
        }
        val oldLastReason = preferences.getString(LAST_SYNC_REASON, null)
        val oldLaunchReason = preferences.getString(LAUNCH_GATE_REASON, null)
        val cleanLastReason = SyncMessages.sanitizeLegacyPrelaunchReason(oldLastReason)
        val cleanLaunchReason = SyncMessages.sanitizeLegacyPrelaunchReason(oldLaunchReason)
        val cleanLastSyncSummary = if (oldLastReason == "prelaunch-remote-head") {
            defaultLastSyncSummary
        } else {
            SyncMessages.sanitizeLegacyUserCopy(
                preferences.getString(LAST_SYNC_SUMMARY, null), defaultLastSyncSummary,
            )
        }
        val cleanLaunchGateSummary = if (oldLaunchReason == "prelaunch-remote-head") {
            defaultLaunchGateSummary
        } else {
            SyncMessages.sanitizeLegacyUserCopy(
                preferences.getString(LAUNCH_GATE_SUMMARY, null), defaultLaunchGateSummary,
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
                .putString(
                    LAST_SYNC_WORKFLOW_STAGE,
                    SaveSyncWorkflowStage.forTransition(
                        cleanLastReason,
                        preferences.getString(LAST_SYNC_PHASE, null).orEmpty(),
                        preferences.getString(LAST_SYNC_ERROR, null).orEmpty(),
                    ).persistedValue,
                )
                .putString(LAUNCH_GATE_REASON, cleanLaunchReason)
                .apply()
        }
    }

    internal fun migrateLegacyWorkManager(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        if (preferences.getInt(LEGACY_WORK_MIGRATION_VERSION, 0) >= 1) return
        val completed = AtomicInteger(0)
        val directExecutor = java.util.concurrent.Executor { runnable -> runnable.run() }
        listOf(LEGACY_PERIODIC_NAME, LEGACY_IMMEDIATE_NAME).forEach { name ->
            val operation = WorkManager.getInstance(context).cancelUniqueWork(name)
            operation.result.addListener(
                {
                    if (runCatching { operation.result.get() }.isSuccess && completed.incrementAndGet() == 2) {
                        preferences.edit().putInt(LEGACY_WORK_MIGRATION_VERSION, 1).commit()
                    }
                },
                directExecutor,
            )
        }
    }

    fun ensurePeriodic(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val captureConstraints = captureConstraints(
            preferences.getBoolean(CHARGING_REQUIRED, false),
        )
        val request = PeriodicWorkRequestBuilder<CaptureWorker>(15, TimeUnit.MINUTES)
            .setConstraints(captureConstraints)
            .setInputData(Data.Builder().putString("reason", "periodic").build())
            .build()
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(
            PERIODIC_NAME, ExistingPeriodicWorkPolicy.UPDATE, request,
        )
    }

    fun enqueueCapture(context: Context, reason: String) {
        val request = OneTimeWorkRequestBuilder<CaptureWorker>()
            .setInputData(Data.Builder().putString("reason", reason).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            CAPTURE_NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request,
        )
    }

    fun enqueueConstrainedDrain(context: Context) {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val constraints = drainConstraints(
            wifiOnly = preferences.getBoolean(WIFI_ONLY, true),
            chargingRequired = preferences.getBoolean(CHARGING_REQUIRED, false),
        )
        val request = OneTimeWorkRequestBuilder<DrainWorker>()
            .setConstraints(constraints)
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            DRAIN_NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request,
        )
    }

    internal fun queueRoot(context: Context) = context.filesDir.resolve("durable-upload-queue-v1")

    fun markDirty(context: Context): Long {
        val generation = NativeSyncBridge.markCaptureDirty(
            queueRoot(context).absolutePath,
            SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
        )
        check(generation >= 0) { "local capture state unavailable" }
        return generation
    }
}
