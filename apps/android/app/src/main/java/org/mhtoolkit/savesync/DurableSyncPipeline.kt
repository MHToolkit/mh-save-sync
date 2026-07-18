package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

internal object AutomaticCapturePolicy {
    fun shouldCapture(reason: String, dirty: Boolean, sessionActive: Boolean): Boolean = when (reason) {
        "save-complete" -> dirty
        "session-exit" -> dirty && !sessionActive
        "manual-sync", "manual-upload" -> !sessionActive
        "periodic" -> dirty && !sessionActive
        else -> false
    }
}

internal data class DurableQueueResult(
    val snapshotId: String,
    val pendingCount: Int,
    val fileCount: Int,
    val totalBytes: Long,
) {
    companion object {
        fun parse(raw: String): DurableQueueResult {
            val json = JSONObject(raw)
            check(!json.has("error")) { "queue_failed" }
            return DurableQueueResult(
                snapshotId = json.getString("snapshot_id"),
                pendingCount = json.getInt("pending_count"),
                fileCount = json.getInt("file_count"),
                totalBytes = json.getLong("total_bytes"),
            )
        }
    }
}

internal data class DurableDrainResult(
    val uploadedCount: Int,
    val conflictCount: Int,
    val failedCount: Int,
    val pendingCount: Int,
    val lastSnapshotId: String?,
    val lastCloudHead: String?,
    val lastError: String?,
) {
    val shouldRetry: Boolean get() = pendingCount > 0

    companion object {
        fun parse(raw: String): DurableDrainResult {
            val json = JSONObject(raw)
            return DurableDrainResult(
                uploadedCount = json.optInt("uploaded_count"),
                conflictCount = json.optInt("conflict_count"),
                failedCount = json.optInt("failed_count"),
                pendingCount = json.optInt("pending_count"),
                lastSnapshotId = json.optString("last_snapshot_id").ifBlank { null },
                lastCloudHead = json.optString("last_cloud_head").ifBlank { null },
                lastError = json.optString("last_error").ifBlank { null },
            )
        }
    }
}

internal data class DurableSyncRunResult(
    val queued: Boolean,
    val uploadedCount: Int,
    val conflictCount: Int,
    val pendingCount: Int,
    val shouldRetry: Boolean,
    val localError: String? = null,
)

internal class DurableSyncPipeline(private val context: Context) {
    suspend fun execute(
        reason: String,
        treeUri: Uri?,
        serverEndpoint: String,
        dirty: Boolean,
        sessionActive: Boolean,
    ): DurableSyncRunResult = withContext(Dispatchers.IO) {
        val server = SyncServerProbe.normalizeServer(serverEndpoint)
        if (server.isBlank()) {
            return@withContext DurableSyncRunResult(false, 0, 0, 0, false, "server_required")
        }
        if (!AndroidSecretVault(context).hasSecret()) {
            return@withContext DurableSyncRunResult(false, 0, 0, 0, false, "recovery_secret_required")
        }
        val queueRoot = context.filesDir.resolve("durable-upload-queue-v1")
        check(queueRoot.exists() || queueRoot.mkdirs()) { "local_queue_unavailable" }
        val deviceId = SyncServerProbe.deviceId(context)
        var secret: ByteArray? = null
        try {
            secret = AndroidSecretVault(context).load()
            val activeSecret = requireNotNull(secret)
            var queued: DurableQueueResult? = null
            var localFingerprint: String? = null
            var localError: String? = null
            if (AutomaticCapturePolicy.shouldCapture(reason, dirty, sessionActive)) {
                val authorizedTree = treeUri
                if (authorizedTree == null) {
                    localError = "saf_permission_required"
                } else if (
                    reason != "save-complete" &&
                    runCatching { NemessixProcessGate(context).requireStopped() }.isFailure
                ) {
                    localError = "emulator_running"
                } else {
                    try {
                        val stage = SafStableStager(context).capture(authorizedTree)
                        try {
                            localFingerprint = stage.fingerprint
                            val binding = SyncConsistencyBinding(
                                serverEndpoint = server,
                                logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                                treeUri = authorizedTree.toString(),
                                deviceId = deviceId,
                            )
                            val baseline = SyncConsistencyLedgerStore(context).read()
                                ?.takeIf { it.binding == binding }
                                ?.establishedRemoteHead
                            val observedBase = baseline ?: runCatching {
                                SyncServerProbe.fetchHeadForReplace(context, server)
                            }.getOrNull()
                            queued = DurableQueueResult.parse(
                                NativeSyncBridge.queueStableStage(
                                    stagingRoot = stage.root.absolutePath,
                                    queueRoot = queueRoot.absolutePath,
                                    serverEndpoint = server,
                                    recoverySecret = activeSecret,
                                    logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                                    baseHead = observedBase,
                                    deviceId = deviceId,
                                ),
                            )
                        } finally {
                            stage.root.deleteRecursively()
                        }
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (_: Exception) {
                        localError = "capture_or_queue_failed"
                    }
                }
            }
            val after = DurableDrainResult.parse(
                NativeSyncBridge.drainUploadQueue(queueRoot.absolutePath, server, activeSecret),
            )
            if (
                queued != null && localFingerprint != null && after.pendingCount == 0 &&
                after.lastSnapshotId == queued.snapshotId && after.lastCloudHead == queued.snapshotId &&
                after.conflictCount == 0
            ) {
                SyncConsistencyLedgerStore(context).establish(
                    binding = SyncConsistencyBinding(
                        serverEndpoint = server,
                        logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                        treeUri = requireNotNull(treeUri).toString(),
                        deviceId = deviceId,
                    ),
                    remoteHead = queued.snapshotId,
                    localFingerprint = localFingerprint,
                    mode = SyncEstablishmentMode.UPLOAD,
                )
            }
            DurableSyncRunResult(
                queued = queued != null,
                uploadedCount = after.uploadedCount,
                conflictCount = after.conflictCount,
                pendingCount = after.pendingCount,
                shouldRetry = after.shouldRetry || localError in setOf(
                    "capture_or_queue_failed",
                    "emulator_running",
                ),
                localError = localError,
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            DurableSyncRunResult(false, 0, 0, 0, false, "local_pipeline_failed")
        } finally {
            secret?.fill(0)
        }
    }
}
