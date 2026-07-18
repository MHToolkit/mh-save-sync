package org.mhtoolkit.savesync

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.IBinder

class ActiveSessionService : Service() {
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
            .setContentTitle(SyncMessages.activeSessionNotificationTitle())
            .setContentText(SyncMessages.activeSessionNotificationText())
            .setOngoing(true)
            .build()
        startForeground(NOTIFICATION_ID, notification)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        SyncScheduler.enqueueImmediate(this, "session-exit")
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private companion object {
        const val CHANNEL = "mh-save-sync-active-session"
        const val NOTIFICATION_ID = 41
    }
}
