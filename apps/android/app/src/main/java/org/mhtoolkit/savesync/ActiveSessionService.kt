package org.mhtoolkit.savesync

import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.IBinder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

internal data class ProcessObservationState(
    val observedRunning: Boolean = false,
    val consecutiveMissing: Int = 0,
) {
    fun next(running: Boolean): ProcessObservationState = when {
        running -> ProcessObservationState(observedRunning = true, consecutiveMissing = 0)
        observedRunning -> copy(consecutiveMissing = consecutiveMissing + 1)
        else -> this
    }

    fun confirmedExit(requiredMissing: Int): Boolean =
        observedRunning && consecutiveMissing >= requiredMissing
}

internal object NemessixProcessEvidence {
    fun matches(processName: String, packages: Collection<String>): Boolean =
        processName == SyncScheduler.NEMESSIX_PACKAGE ||
            processName.startsWith("${SyncScheduler.NEMESSIX_PACKAGE}:") ||
            SyncScheduler.NEMESSIX_PACKAGE in packages
}

class ActiveSessionService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var monitor: Job? = null

    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL,
                SyncMessages.activeSessionChannelName(),
                NotificationManager.IMPORTANCE_LOW,
            ),
        )
        val notification = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_save_sync)
            .setContentTitle("正在跟踪 Nemessix 游玩会话")
            .setContentText("未自动识别退出时，请返回 MH 云存档明确确认已退出")
            .setOngoing(true)
            .build()
        startForeground(NOTIFICATION_ID, notification)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_MANUAL_STOP) {
            finishSession("manual-session-exit")
            return START_NOT_STICKY
        }
        getSharedPreferences(SyncScheduler.PREFERENCES, MODE_PRIVATE)
            .edit().putBoolean(SyncScheduler.SESSION_ACTIVE, true).commit()
        SyncScheduler.markDirty(this)
        if (SyncScheduler.PROCESS_EXIT_RUNTIME_VERIFIED) {
            startProcessMonitor()
        }
        return START_STICKY
    }

    private fun startProcessMonitor() {
        if (monitor?.isActive == true) return
        monitor = scope.launch {
            var observation = ProcessObservationState()
            while (isActive) {
                observation = observation.next(isNemessixProcessVisible())
                if (observation.confirmedExit(REQUIRED_MISSING_POLLS)) {
                    finishSession("session-exit")
                    break
                }
                delay(POLL_INTERVAL_MS)
            }
        }
    }

    private fun isNemessixProcessVisible(): Boolean {
        val manager = getSystemService(ActivityManager::class.java)
        return manager.runningAppProcesses.orEmpty().any { process ->
            NemessixProcessEvidence.matches(process.processName, process.pkgList.orEmpty().asList())
        }
    }

    private fun finishSession(reason: String) {
        getSharedPreferences(SyncScheduler.PREFERENCES, MODE_PRIVATE)
            .edit()
            .putBoolean(SyncScheduler.SESSION_ACTIVE, false)
            .putString(SyncScheduler.LAST_SYNC_REASON, reason)
            .putString(SyncScheduler.LAST_SYNC_PHASE, SyncMessages.queuedPhase(reason))
            .putString(
                SyncScheduler.LAST_SYNC_WORKFLOW_STAGE,
                SaveSyncWorkflowStage.forTransition(reason, SyncMessages.queuedPhase(reason), "").persistedValue,
            )
            .putString(SyncScheduler.LAST_SYNC_ERROR, "")
            .commit()
        SyncScheduler.markDirty(this)
        SyncScheduler.enqueueCapture(this, "session-exit")
        stopSelf()
    }

    override fun onDestroy() {
        monitor?.cancel()
        scope.cancel()
        // Service destruction is not proof that Nemessix exited. Never enqueue from here.
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        const val ACTION_TRACK_LAUNCH = "org.mhtoolkit.savesync.TRACK_NEMESSIX_LAUNCH"
        const val ACTION_MANUAL_STOP = "org.mhtoolkit.savesync.MANUAL_SESSION_STOP"
        private const val CHANNEL = "mh-save-sync-active-session"
        private const val NOTIFICATION_ID = 41
        private const val POLL_INTERVAL_MS = 2_000L
        private const val REQUIRED_MISSING_POLLS = 3
    }
}
