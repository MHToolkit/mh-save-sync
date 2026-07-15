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
            val current = SyncServerProbe.fetchHeadForReplace(context, server)
            val base = LocalReplacePolicy.requireObservedBase(observedBase, current)
            val stage = SafStableStager(context).capture(treeUri)
            var secret: ByteArray? = null
            try {
                val normalizedServer = SyncServerProbe.normalizeServer(server)
                val device = deviceId()
                secret = AndroidSecretVault(context).load()
                val output = NativeSyncBridge.uploadStableStage(
                    stagingRoot = stage.root.absolutePath,
                    serverEndpoint = normalizedServer,
                    recoverySecret = secret,
                    logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                    baseHead = base,
                    deviceId = device,
                )
                val result = LocalReplaceResult.parse(output)
                if (SyncLedgerWritePolicy.shouldEstablishAfterUpload(result)) {
                    result as LocalReplaceResult.Uploaded
                        val confirmedHead = runCatching {
                            SyncServerProbe.fetchHeadForReplace(context, normalizedServer)
                        }.getOrNull()
                        // The uploaded immutable snapshot was built from this exact stable stage.
                        // A ledger failure must not turn a completed cloud commit into a false
                        // upload failure; the old baseline remains conservative on next check.
                        val established = UploadConsistencyPolicy.canEstablish(result, confirmedHead) && runCatching {
                            SyncConsistencyLedgerStore(context).establish(
                                binding = SyncConsistencyBinding(
                                    serverEndpoint = normalizedServer,
                                    logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                                    treeUri = treeUri.toString(),
                                    deviceId = device,
                                ),
                                remoteHead = result.cloudHead,
                                localFingerprint = stage.fingerprint,
                                mode = SyncEstablishmentMode.UPLOAD,
                            )
                            true
                        }.getOrDefault(false)
                        result.copy(consistencyEstablished = established)
                } else {
                    result
                }
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
