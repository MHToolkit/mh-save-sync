package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import org.json.JSONObject

internal object SafGrantPolicy {
    fun isUsable(root: String?, readablePersistedUris: Set<String>): Boolean =
        root != null && root in readablePersistedUris
}

internal sealed interface SafGrantInspection {
    data class Available(val readableUris: Set<String>) : SafGrantInspection
    data object Revoked : SafGrantInspection

    companion object {
        fun inspect(loader: () -> Set<String>): SafGrantInspection = try {
            Available(loader())
        } catch (_: SecurityException) {
            Revoked
        }
    }
}

internal data class CaptureGenerationClaim(val generation: Long, val owner: String) {
    companion object {
        fun parse(raw: String): CaptureGenerationClaim? {
            val json = JSONObject(raw)
            check(!json.has("error")) { "capture_claim_failed" }
            if (!json.optBoolean("claimed")) return null
            return CaptureGenerationClaim(json.getLong("generation"), json.getString("owner"))
        }
    }
}

internal enum class CaptureFailureDisposition { RETRY_QUEUE_UNKNOWN, REAUTHORIZE, RETRY_CAPTURE, COMPLETE }

internal object CaptureFailurePolicy {
    fun decide(localError: String?, leaseReleased: Boolean): CaptureFailureDisposition = when {
        !leaseReleased -> CaptureFailureDisposition.RETRY_QUEUE_UNKNOWN
        localError == "saf_permission_required" -> CaptureFailureDisposition.REAUTHORIZE
        localError in setOf("emulator_running", "capture_or_queue_failed") ->
            CaptureFailureDisposition.RETRY_CAPTURE
        else -> CaptureFailureDisposition.COMPLETE
    }

    fun shouldRemoveSafRoot(localError: String?): Boolean =
        localError == "saf_permission_required"
}

internal data class DrainUiStatus(
    val phase: String,
    val summary: String,
    val nextAction: String,
    val error: String,
)

internal object DrainStatusPolicy {
    fun decide(result: DurableDrainResult): DrainUiStatus {
        val multipleEndpoints = result.pendingEndpointCount > 1
        return when {
            result.lastError == "recovery_secret_required" -> DrainUiStatus(
                "需要恢复密钥",
                "上传队列保留在手机上，但需要恢复密钥才能继续。",
                "导入与其他设备相同的恢复密钥。",
                "未导入恢复密钥",
            )
            result.conflictCount > 0 -> DrainUiStatus(
                "检测到冲突",
                if (result.pendingCount > 0) {
                    "已保留冲突分支；另有加密任务等待网络续传，没有静默覆盖。"
                } else {
                    "手机快照已安全上传为冲突分支，没有静默覆盖云端或本地存档。"
                },
                "打开冲突页面明确选择；其余任务会按约束重试。",
                "",
            )
            !result.queueStateKnown || result.lastError in setOf(
                "local_queue_unavailable",
                "local_pipeline_failed",
            ) -> DrainUiStatus(
                "本地上传队列暂不可用",
                "无法确认本地加密队列状态；不会把未知状态显示为上传完成。",
                "后台会重试；不要清除应用数据。",
                "本地队列读取失败",
            )
            result.lastError == "local_queue_integrity_failure" -> DrainUiStatus(
                "本地加密任务校验失败",
                "至少一个队列项目未通过完整性校验；其他原始存档和队列未被覆盖。",
                "保留应用数据并查看诊断；后台不会把损坏项目伪报为成功。",
                "队列完整性校验失败",
            )
            result.pendingCount > 0 || result.lastError == "network_or_server_failure" -> DrainUiStatus(
                "离线队列待续传",
                if (multipleEndpoints) {
                    "队列包含多个原服务器地址；当前不可用地址不会阻塞其他地址。"
                } else {
                    "加密快照仍在手机持久队列中，网络或云端不可用不会丢失任务。"
                },
                "满足 Wi-Fi、电量和充电偏好后会自动重试。",
                "云端暂时不可用",
            )
            result.uploadedCount > 0 -> DrainUiStatus(
                "上传完成",
                "Android 稳定快照已加密上传到云存档服务器。",
                "可以在 Mac 端检查并恢复这个版本。",
                "",
            )
            else -> DrainUiStatus(
                "后台对账完成",
                "本地加密上传队列当前为空，没有声称已观察到 Nemessix 自动退出。",
                "退出游戏后返回本应用，点“尝试读取稳定存档并排队上传”。",
                "",
            )
        }
    }
}

private data class PendingLedgerReceipt(
    val snapshotId: String,
    val serverEndpoint: String,
    val treeUri: String,
    val deviceId: String,
    val fingerprint: String,
)

private object PendingLedgerReceiptStore {
    private const val KEY = "pending_upload_ledger_receipt"

    fun write(context: Context, receipt: PendingLedgerReceipt) {
        val json = JSONObject()
            .put("snapshot_id", receipt.snapshotId)
            .put("server_endpoint", receipt.serverEndpoint)
            .put("tree_uri", receipt.treeUri)
            .put("device_id", receipt.deviceId)
            .put("fingerprint", receipt.fingerprint)
        context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
            .edit().putString(KEY, json.toString()).commit()
    }

    fun read(context: Context): PendingLedgerReceipt? = runCatching {
        val raw = context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
            .getString(KEY, null) ?: return null
        val json = JSONObject(raw)
        PendingLedgerReceipt(
            snapshotId = json.getString("snapshot_id"),
            serverEndpoint = json.getString("server_endpoint"),
            treeUri = json.getString("tree_uri"),
            deviceId = json.getString("device_id"),
            fingerprint = json.getString("fingerprint"),
        )
    }.getOrNull()

    fun clear(context: Context) {
        context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
            .edit().remove(KEY).apply()
    }
}

open class CaptureWorker(
    appContext: Context,
    params: WorkerParameters,
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        val preferences = applicationContext.getSharedPreferences(
            SyncScheduler.PREFERENCES, Context.MODE_PRIVATE,
        )
        if (!preferences.getBoolean(SyncScheduler.GAME_MH3G_ENABLED, true)) {
            return Result.success()
        }
        val reason = inputData.getString("reason") ?: "periodic"
        val endpoint = preferences.getString(SyncScheduler.SERVER_ENDPOINT, null).orEmpty()
        val root = preferences.getString(SyncScheduler.SAF_ROOT, null)
        val grantInspection = SafGrantInspection.inspect {
            applicationContext.contentResolver.persistedUriPermissions
                .filter { it.isReadPermission }
                .map { it.uri.toString() }
                .toSet()
        }
        if (grantInspection is SafGrantInspection.Revoked) {
            preferences.edit().remove(SyncScheduler.SAF_ROOT).apply()
            writeSafRevokedStatus(preferences, reason)
            return Result.success()
        }
        val readablePersistedUris = (grantInspection as SafGrantInspection.Available).readableUris
        val stillGranted = SafGrantPolicy.isUsable(root, readablePersistedUris)
        if (root != null && !stillGranted) {
            preferences.edit().remove(SyncScheduler.SAF_ROOT).apply()
            writeSafRevokedStatus(preferences, reason)
            return Result.success()
        }
        val sessionActive = preferences.getBoolean(SyncScheduler.SESSION_ACTIVE, false)
        if (!AutomaticCapturePolicy.shouldCapture(reason, dirty = true, sessionActive)) {
            SyncScheduler.enqueueConstrainedDrain(applicationContext)
            return Result.success()
        }
        val queueRoot = SyncScheduler.queueRoot(applicationContext)
        val claim = runCatching {
            CaptureGenerationClaim.parse(
                NativeSyncBridge.claimCaptureGeneration(
                    queueRoot.absolutePath,
                    SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                ),
            )
        }.getOrElse {
            preferences.edit()
                .putString(SyncScheduler.LAST_SYNC_PHASE, "本地队列不可用")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "capture_claim_failed")
                .apply()
            return Result.retry()
        }
        if (claim == null) {
            SyncScheduler.enqueueConstrainedDrain(applicationContext)
            return Result.success()
        }
        val capture = DurableSyncPipeline(applicationContext).capture(
            reason = reason,
            treeUri = root?.takeIf { stillGranted }?.let(Uri::parse),
            serverEndpoint = endpoint,
            sessionActive = sessionActive,
            captureOwner = claim.owner,
            captureGeneration = claim.generation,
        )
        capture.queued?.let { queued ->
            capture.localFingerprint?.let { fingerprint ->
                PendingLedgerReceiptStore.write(
                    applicationContext,
                    PendingLedgerReceipt(
                        snapshotId = queued.snapshotId,
                        serverEndpoint = SyncServerProbe.normalizeServer(endpoint),
                        treeUri = requireNotNull(root),
                        deviceId = SyncServerProbe.deviceId(applicationContext),
                        fingerprint = fingerprint,
                    ),
                )
            }
            preferences.edit()
                .putInt(SyncScheduler.PENDING_UPLOAD_COUNT, queued.pendingCount)
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(SyncScheduler.LAST_SYNC_PHASE, "已保存到本机队列")
                .putString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    "稳定快照已端到端加密并写入手机持久队列，等待符合网络和电量条件后上传。",
                )
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "可以离线离开；后台稍后自动续传。")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "")
                .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
                .apply()
            SyncScheduler.enqueueConstrainedDrain(applicationContext)
            return Result.success()
        }
        val leaseReleased = NativeSyncBridge.finishCaptureGeneration(
            queueRoot.absolutePath,
            SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
            claim.owner,
            claim.generation,
            false,
        )
        val disposition = CaptureFailurePolicy.decide(capture.localError, leaseReleased)
        if (disposition == CaptureFailureDisposition.RETRY_QUEUE_UNKNOWN) {
            preferences.edit()
                .putString(SyncScheduler.LAST_SYNC_PHASE, "本地捕获队列暂不可用")
                .putString(SyncScheduler.LAST_SYNC_SUMMARY, "无法确认捕获租约已释放；不会等待十分钟后静默重复快照。")
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "后台会重试；不要清除应用数据。")
                .putString(SyncScheduler.LAST_SYNC_ERROR, "capture_lease_release_failed")
                .apply()
            return Result.retry()
        }
        if (capture.localError != null) {
            val revoked = disposition == CaptureFailureDisposition.REAUTHORIZE
            if (CaptureFailurePolicy.shouldRemoveSafRoot(capture.localError)) {
                preferences.edit().remove(SyncScheduler.SAF_ROOT).commit()
            }
            preferences.edit()
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(SyncScheduler.LAST_SYNC_PHASE, if (revoked) "需要重新授权" else "本地快照未创建")
                .putString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    if (revoked) "无法读取新的存档候选；已有加密队列未删除。" else
                        "同步安全停止：没有上传不稳定存档，也没有修改本地原始存档。",
                )
                .putString(
                    SyncScheduler.LAST_SYNC_NEXT_ACTION,
                    if (revoked) "请重新选择 Android Nemessix 存档目录。" else "确认 Nemessix 已退出后重试。",
                )
                .putString(SyncScheduler.LAST_SYNC_ERROR, capture.localError)
                .apply()
            return if (disposition == CaptureFailureDisposition.RETRY_CAPTURE) {
                Result.retry()
            } else {
                Result.success()
            }
        }
        // Even when no new capture is needed, an old-endpoint queue remains visible and drainable.
        SyncScheduler.enqueueConstrainedDrain(applicationContext)
        return Result.success()
    }

    private fun writeSafRevokedStatus(
        preferences: android.content.SharedPreferences,
        reason: String,
    ) {
        preferences.edit()
            .putString(SyncScheduler.LAST_SYNC_REASON, reason)
            .putString(SyncScheduler.LAST_SYNC_PHASE, "需要重新授权")
            .putString(SyncScheduler.LAST_SYNC_SUMMARY, "Android 已撤销存档目录权限；没有读取或上传新候选。")
            .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, "请重新选择 Android Nemessix 存档目录。")
            .putString(SyncScheduler.LAST_SYNC_ERROR, "saf_permission_required")
            .apply()
    }
}

class DrainWorker(
    appContext: Context,
    params: WorkerParameters,
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        val preferences = applicationContext.getSharedPreferences(
            SyncScheduler.PREFERENCES, Context.MODE_PRIVATE,
        )
        val result = DurableSyncPipeline(applicationContext).drain()
        val displayedPendingCount = if (result.queueStateKnown) {
            result.pendingCount
        } else {
            preferences.getInt(SyncScheduler.PENDING_UPLOAD_COUNT, 0)
        }
        val receipt = PendingLedgerReceiptStore.read(applicationContext)
        if (
            receipt != null && result.pendingCount == 0 && result.conflictCount == 0 &&
            result.lastSnapshotId == receipt.snapshotId && result.lastCloudHead == receipt.snapshotId
        ) {
            runCatching {
                SyncConsistencyLedgerStore(applicationContext).establish(
                    binding = SyncConsistencyBinding(
                        serverEndpoint = receipt.serverEndpoint,
                        logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                        treeUri = receipt.treeUri,
                        deviceId = receipt.deviceId,
                    ),
                    remoteHead = receipt.snapshotId,
                    localFingerprint = receipt.fingerprint,
                    mode = SyncEstablishmentMode.UPLOAD,
                )
            }.onSuccess { PendingLedgerReceiptStore.clear(applicationContext) }
        }
        val status = DrainStatusPolicy.decide(result)
        preferences.edit()
            .putLong(SyncScheduler.LAST_SYNC_UNIX_MS, System.currentTimeMillis())
            .putString(SyncScheduler.LAST_SYNC_REASON, "constrained-drain")
            .putString(SyncScheduler.LAST_SYNC_SUMMARY, status.summary)
            .putString(SyncScheduler.LAST_SYNC_PHASE, status.phase)
            .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, status.nextAction)
            .putString(SyncScheduler.LAST_SYNC_ERROR, status.error)
            .putInt(SyncScheduler.PENDING_UPLOAD_COUNT, displayedPendingCount)
            .apply()
        if (result.queueStateKnown) {
            preferences.edit()
                .putInt(SyncScheduler.PENDING_UPLOAD_ENDPOINT_COUNT, result.pendingEndpointCount)
                .apply()
        }
        return if (result.shouldRetry) Result.retry() else Result.success()
    }
}

/** Compatibility target for WorkManager rows persisted by alpha.3. It never drains directly. */
class ReconcileWorker(appContext: Context, params: WorkerParameters) : CaptureWorker(appContext, params)
