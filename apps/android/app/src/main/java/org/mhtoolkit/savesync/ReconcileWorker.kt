package org.mhtoolkit.savesync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters

class ReconcileWorker(appContext: Context, params: WorkerParameters) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        // Dirty reconciliation only. Watchers and periodic work must not upload directly.
        return Result.success()
    }
}
