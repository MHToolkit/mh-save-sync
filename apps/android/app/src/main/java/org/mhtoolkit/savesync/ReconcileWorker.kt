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
            ?: return Result.failure()
        val stillGranted = applicationContext.contentResolver.persistedUriPermissions.any {
            it.uri.toString() == root && it.isReadPermission
        }
        if (!stillGranted) {
            preferences.edit().remove(SyncScheduler.SAF_ROOT).apply()
            return Result.failure()
        }
        // The platform shell marks the profile dirty. Shared Rust performs the
        // stable-copy/hash/validate/encrypt/upload pipeline once JNI is loaded.
        preferences.edit()
            .putLong("last_reconcile_unix_ms", System.currentTimeMillis())
            .putString("last_reconcile_reason", inputData.getString("reason") ?: "periodic")
            .apply()
        return Result.success()
    }
}
