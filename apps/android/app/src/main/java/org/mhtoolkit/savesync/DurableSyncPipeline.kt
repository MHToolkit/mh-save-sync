package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject

internal object AutomaticCapturePolicy {
    fun shouldCapture(reason: String, dirty: Boolean, sessionActive: Boolean): Boolean = when (reason) {
        "session-exit", "periodic" -> dirty && !sessionActive
        "manual-sync", "manual-upload" -> !sessionActive
        // No released Nemessix build currently exposes a pinned save-complete event.
        "save-complete" -> false
        else -> false
    }
}

internal object AndroidSyncOperationMutex {
    val value = Mutex()
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
    val pendingEndpointCount: Int,
    val lastSnapshotId: String?,
    val lastCloudHead: String?,
    val lastError: String?,
    val queueStateKnown: Boolean,
) {
    val shouldRetry: Boolean get() = pendingCount > 0 || lastError != null || !queueStateKnown

    companion object {
        fun parse(raw: String): DurableDrainResult {
            val json = JSONObject(raw)
            return DurableDrainResult(
                uploadedCount = json.optInt("uploaded_count"),
                conflictCount = json.optInt("conflict_count"),
                failedCount = json.optInt("failed_count"),
                pendingCount = json.optInt("pending_count"),
                pendingEndpointCount = json.optInt("pending_endpoint_count"),
                lastSnapshotId = json.optString("last_snapshot_id").ifBlank { null },
                lastCloudHead = json.optString("last_cloud_head").ifBlank { null },
                lastError = json.optString("last_error").ifBlank { null },
                queueStateKnown = json.optBoolean("queue_state_known", false),
            )
        }
    }
}

internal data class DurableCaptureResult(
    val queued: DurableQueueResult? = null,
    val localFingerprint: String? = null,
    val localError: String? = null,
)

internal class DurableSyncPipeline(private val context: Context) {
    suspend fun capture(
        reason: String,
        treeUri: Uri?,
        serverEndpoint: String,
        sessionActive: Boolean,
        captureOwner: String,
        captureGeneration: Long,
    ): DurableCaptureResult = withContext(Dispatchers.IO) {
        AndroidSyncOperationMutex.value.withLock {
            if (!AutomaticCapturePolicy.shouldCapture(reason, dirty = true, sessionActive)) {
                return@withLock DurableCaptureResult()
            }
            val server = SyncServerProbe.normalizeServer(serverEndpoint)
            if (server.isBlank()) return@withLock DurableCaptureResult(localError = "server_required")
            if (!AndroidSecretVault(context).hasSecret()) {
                return@withLock DurableCaptureResult(localError = "recovery_secret_required")
            }
            val authorizedTree = treeUri
                ?: return@withLock DurableCaptureResult(localError = "saf_permission_required")
            if (runCatching { NemessixProcessGate(context).requireStopped() }.isFailure) {
                return@withLock DurableCaptureResult(localError = "emulator_running")
            }
            val queueRoot = context.filesDir.resolve("durable-upload-queue-v1")
            if (!(queueRoot.exists() || queueRoot.mkdirs())) {
                return@withLock DurableCaptureResult(localError = "local_queue_unavailable")
            }
            var secret: ByteArray? = null
            try {
                secret = AndroidSecretVault(context).load()
                val stage = SafStableStager(context).capture(authorizedTree)
                try {
                    val deviceId = SyncServerProbe.deviceId(context)
                    val binding = SyncConsistencyBinding(
                        serverEndpoint = server,
                        logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                        treeUri = authorizedTree.toString(),
                        deviceId = deviceId,
                    )
                    // Capture is deliberately offline-only. Never probe the server here: the
                    // immutable queue item keeps its original endpoint and base for later drain.
                    val baseline = runCatching {
                        SyncConsistencyLedgerCodec.decode(
                            NativeSyncBridge.readConsistencyBaseline(
                                queueRoot.absolutePath,
                                server,
                                binding.logicalSaveId,
                                binding.treeUri,
                                binding.deviceId,
                            ),
                        )
                    }.getOrNull()
                        ?.takeIf { it.binding == binding }
                        ?.establishedRemoteHead
                        ?: SyncConsistencyLedgerStore(context).read()
                            ?.takeIf { it.binding == binding }
                            ?.establishedRemoteHead
                    val queued = DurableQueueResult.parse(
                        NativeSyncBridge.queueStableStage(
                            stagingRoot = stage.root.absolutePath,
                            queueRoot = queueRoot.absolutePath,
                            serverEndpoint = server,
                            recoverySecret = requireNotNull(secret),
                            logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                            baseHead = baseline,
                            deviceId = deviceId,
                            treeUri = binding.treeUri,
                            localFingerprint = stage.fingerprint,
                            captureOwner = captureOwner,
                            captureGeneration = captureGeneration,
                        ),
                    )
                    DurableCaptureResult(queued, stage.fingerprint)
                } finally {
                    stage.root.deleteRecursively()
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: SecurityException) {
                DurableCaptureResult(localError = "saf_permission_required")
            } catch (_: Exception) {
                DurableCaptureResult(localError = "capture_or_queue_failed")
            } finally {
                secret?.fill(0)
            }
        }
    }

    suspend fun drain(): DurableDrainResult = withContext(Dispatchers.IO) {
        AndroidSyncOperationMutex.value.withLock {
            var secret: ByteArray? = null
            try {
                if (!AndroidSecretVault(context).hasSecret()) {
                    val stateExists = SyncScheduler.queueRoot(context).resolve("state.sqlite").isFile
                    return@withLock if (stateExists) {
                        DurableDrainResult(0, 0, 0, 0, 0, null, null, "recovery_secret_required", false)
                    } else {
                        DurableDrainResult(0, 0, 0, 0, 0, null, null, null, true)
                    }
                }
                secret = AndroidSecretVault(context).load()
                val queueRoot = context.filesDir.resolve("durable-upload-queue-v1")
                DurableDrainResult.parse(
                    NativeSyncBridge.drainUploadQueue(queueRoot.absolutePath, requireNotNull(secret)),
                )
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Exception) {
                DurableDrainResult(0, 0, 1, 0, 0, null, null, "local_pipeline_failed", false)
            } finally {
                secret?.fill(0)
            }
        }
    }
}
