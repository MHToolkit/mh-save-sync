package org.mhtoolkit.savesync

import android.app.ActivityManager
import android.content.Context
import android.net.Uri
import android.provider.Settings
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class NemessixProcessGate(private val context: Context) {
    fun requireStopped() {
        val manager = context.getSystemService(ActivityManager::class.java)
        val running = manager.runningAppProcesses.orEmpty().any { process ->
            process.processName == SyncScheduler.NEMESSIX_PACKAGE ||
                process.pkgList?.contains(SyncScheduler.NEMESSIX_PACKAGE) == true
        }
        check(!running) { "Nemessix 仍在运行。请从最近任务划掉并等待退出后重试" }
    }
}

class LocalReplacePipeline(private val context: Context) {
    suspend fun execute(
        server: String,
        treeUri: Uri,
        observedBase: String?,
        sessionActive: Boolean,
    ): LocalReplaceResult =
        withContext(Dispatchers.IO) {
            LocalReplacePolicy.requireSessionStopped(sessionActive)
            NemessixProcessGate(context).requireStopped()
            val current = SyncServerProbe.fetchHeadForReplace(server)
            val base = LocalReplacePolicy.requireObservedBase(observedBase, current)
            val stage = SafStableStager(context).capture(treeUri)
            var secret: ByteArray? = null
            try {
                secret = AndroidSecretVault(context).load()
                val output = NativeSyncBridge.uploadStableStage(
                    stagingRoot = stage.root.absolutePath,
                    serverEndpoint = SyncServerProbe.normalizeServer(server),
                    recoverySecret = secret,
                    logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                    baseHead = base,
                    deviceId = deviceId(),
                )
                LocalReplaceResult.parse(output)
            } finally {
                secret?.fill(0)
                stage.root.deleteRecursively()
            }
        }

    private fun deviceId(): String {
        val raw = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID)
            .orEmpty().toByteArray()
        return try {
            "android-" + MessageDigest.getInstance("SHA-256").digest(raw)
                .take(8).joinToString("") { "%02x".format(it) }
        } finally {
            raw.fill(0)
        }
    }
}
