package org.mhtoolkit.savesync

import android.content.Context
import kotlinx.coroutines.CancellationException
import org.json.JSONObject

data class SyncConsistencyBinding(
    val serverEndpoint: String,
    val logicalSaveId: String,
    val treeUri: String,
    val deviceId: String,
)

enum class SyncEstablishmentMode { UPLOAD, RESTORE }

data class SyncConsistencyBaseline(
    val binding: SyncConsistencyBinding,
    val establishedRemoteHead: String,
    val localFingerprint: String,
    val establishedAtMillis: Long,
    val mode: SyncEstablishmentMode,
)

sealed interface LocalFingerprintObservation {
    data class Available(val fingerprint: String) : LocalFingerprintObservation
    data object Unavailable : LocalFingerprintObservation
}

enum class PrelaunchConsistencyState(val reason: String) {
    SYNCED("prelaunch-synced"),
    REMOTE_ADVANCED("prelaunch-remote-advanced"),
    LOCAL_CHANGED("prelaunch-local-changed"),
    DIVERGED("prelaunch-diverged"),
    UNKNOWN("prelaunch-unknown"),
    LOCAL_UNAVAILABLE("prelaunch-local-unavailable"),
    CLOUD_UNAVAILABLE("prelaunch-cloud-unavailable"),
    NO_REMOTE("prelaunch-no-remote-head"),
    NO_SERVER("prelaunch-no-server"),
    KEY_REQUIRED("prelaunch-key-required"),
    EMULATOR_RUNNING("prelaunch-emulator-running"),
}

internal object PrelaunchConsistencyPolicy {
    fun classify(
        binding: SyncConsistencyBinding,
        baseline: SyncConsistencyBaseline?,
        localFingerprint: LocalFingerprintObservation,
        remoteHead: String?,
        emulatorRunning: Boolean,
    ): PrelaunchConsistencyState {
        if (emulatorRunning) return PrelaunchConsistencyState.EMULATOR_RUNNING
        if (localFingerprint is LocalFingerprintObservation.Unavailable) {
            return PrelaunchConsistencyState.LOCAL_UNAVAILABLE
        }
        localFingerprint as LocalFingerprintObservation.Available
        if (baseline == null) {
            return if (remoteHead == null) {
                PrelaunchConsistencyState.NO_REMOTE
            } else {
                PrelaunchConsistencyState.UNKNOWN
            }
        }
        if (baseline.binding != binding || remoteHead == null) {
            return PrelaunchConsistencyState.UNKNOWN
        }
        val localChanged = localFingerprint.fingerprint != baseline.localFingerprint
        val remoteChanged = remoteHead != baseline.establishedRemoteHead
        return when {
            localChanged && remoteChanged -> PrelaunchConsistencyState.DIVERGED
            localChanged -> PrelaunchConsistencyState.LOCAL_CHANGED
            remoteChanged -> PrelaunchConsistencyState.REMOTE_ADVANCED
            else -> PrelaunchConsistencyState.SYNCED
        }
    }
}

internal object PrelaunchCapturePolicy {
    fun shouldCaptureLocal(emulatorRunning: Boolean): Boolean = !emulatorRunning
}

internal object SyncLedgerWritePolicy {
    fun shouldEstablishAfterUpload(result: LocalReplaceResult): Boolean =
        result is LocalReplaceResult.Uploaded
}

internal object UploadConsistencyPolicy {
    private val SNAPSHOT_ID = Regex("[0-9a-f]{64}")

    fun canEstablish(result: LocalReplaceResult.Uploaded, confirmedRemoteHead: String?): Boolean =
        result.snapshotId.matches(SNAPSHOT_ID) &&
            result.cloudHead.matches(SNAPSHOT_ID) &&
            result.snapshotId == result.cloudHead &&
            confirmedRemoteHead == result.snapshotId
}

internal data class StablePrelaunchObservation(
    val localFingerprint: String,
    val remoteHead: String?,
)

internal class LocalCaptureUnavailableException(cause: Throwable) : RuntimeException(cause)

internal object PrelaunchObservationCoordinator {
    suspend fun captureThenRefetch(
        captureLocal: suspend () -> String,
        refetchRemoteHead: suspend () -> String?,
    ): StablePrelaunchObservation {
        val fingerprint = try {
            captureLocal()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            throw LocalCaptureUnavailableException(error)
        }
        return StablePrelaunchObservation(fingerprint, refetchRemoteHead())
    }
}

internal object RestoreConsistencyCoordinator {
    suspend fun complete(
        captureAndEstablish: suspend () -> Unit,
        releaseLease: () -> Unit,
    ): Boolean {
        var established = false
        try {
            try {
                captureAndEstablish()
                established = true
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Exception) {
                established = false
            }
        } finally {
            releaseLease()
        }
        return established
    }
}

internal object PrelaunchLaunchPolicy {
    fun launchAutomatically(state: PrelaunchConsistencyState): Boolean =
        state == PrelaunchConsistencyState.SYNCED || state == PrelaunchConsistencyState.NO_REMOTE

    fun allowExplicitLocalLaunch(state: PrelaunchConsistencyState): Boolean = state in setOf(
        PrelaunchConsistencyState.REMOTE_ADVANCED,
        PrelaunchConsistencyState.LOCAL_CHANGED,
        PrelaunchConsistencyState.DIVERGED,
        PrelaunchConsistencyState.UNKNOWN,
        PrelaunchConsistencyState.LOCAL_UNAVAILABLE,
        PrelaunchConsistencyState.CLOUD_UNAVAILABLE,
        PrelaunchConsistencyState.NO_SERVER,
        PrelaunchConsistencyState.KEY_REQUIRED,
    )

    fun allowCloudRestore(state: PrelaunchConsistencyState): Boolean =
        state != PrelaunchConsistencyState.EMULATOR_RUNNING
}

internal object SyncConsistencyLedgerCodec {
    private const val SCHEMA_VERSION = 1

    fun encode(value: SyncConsistencyBaseline): String = JSONObject()
        .put("schema_version", SCHEMA_VERSION)
        .put("server_endpoint", value.binding.serverEndpoint)
        .put("logical_save_id", value.binding.logicalSaveId)
        .put("tree_uri", value.binding.treeUri)
        .put("device_id", value.binding.deviceId)
        .put("established_remote_head", value.establishedRemoteHead)
        .put("local_fingerprint", value.localFingerprint)
        .put("established_at_millis", value.establishedAtMillis)
        .put("mode", value.mode.name.lowercase())
        .toString()

    fun decode(raw: String?): SyncConsistencyBaseline? = runCatching {
        val json = JSONObject(raw ?: return null)
        require(json.getInt("schema_version") == SCHEMA_VERSION)
        val binding = SyncConsistencyBinding(
            serverEndpoint = json.getString("server_endpoint"),
            logicalSaveId = json.getString("logical_save_id"),
            treeUri = json.getString("tree_uri"),
            deviceId = json.getString("device_id"),
        )
        val baseline = SyncConsistencyBaseline(
            binding = binding,
            establishedRemoteHead = json.getString("established_remote_head"),
            localFingerprint = json.getString("local_fingerprint"),
            establishedAtMillis = json.getLong("established_at_millis"),
            mode = SyncEstablishmentMode.valueOf(json.getString("mode").uppercase()),
        )
        require(binding.serverEndpoint.isNotBlank())
        require(binding.logicalSaveId.isNotBlank())
        require(binding.treeUri.isNotBlank())
        require(binding.deviceId.isNotBlank())
        require(baseline.establishedRemoteHead.isNotBlank())
        require(baseline.localFingerprint.isNotBlank())
        require(baseline.establishedAtMillis > 0)
        baseline
    }.getOrNull()
}

class SyncConsistencyLedgerStore(private val context: Context) {
    fun read(): SyncConsistencyBaseline? = SyncConsistencyLedgerCodec.decode(
        context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
            .getString(KEY, null),
    )

    fun establish(
        binding: SyncConsistencyBinding,
        remoteHead: String,
        localFingerprint: String,
        mode: SyncEstablishmentMode,
        establishedAtMillis: Long = System.currentTimeMillis(),
    ) {
        val value = SyncConsistencyBaseline(
            binding = binding,
            establishedRemoteHead = remoteHead,
            localFingerprint = localFingerprint,
            establishedAtMillis = establishedAtMillis,
            mode = mode,
        )
        val encoded = SyncConsistencyLedgerCodec.encode(value)
        check(
            context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE)
                .edit().putString(KEY, encoded).commit(),
        ) { "sync_consistency_ledger_commit_failed" }
        check(read() == value) { "sync_consistency_ledger_verify_failed" }
    }

    private companion object {
        const val KEY = "sync_consistency_ledger_v1"
    }
}
