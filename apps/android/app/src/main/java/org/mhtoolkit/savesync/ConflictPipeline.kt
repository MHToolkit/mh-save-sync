package org.mhtoolkit.savesync

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

data class ConflictBranchSummary(
    val snapshotId: String,
    val cloudHead: String,
    val branchDeviceId: String,
    val branchCreatedUnixMs: Long,
    val cloudDeviceId: String,
    val cloudCreatedUnixMs: Long,
    val changedFiles: Long,
    val changedBytes: Long,
)

data class UnresolvedConflictReport(
    val cloudHead: String?,
    val branches: List<ConflictBranchSummary>,
)

data class ConflictResolutionReport(val resolved: Int, val total: Int) {
    val complete: Boolean get() = resolved == total
}

object ConflictReportParser {
    fun parse(raw: String): UnresolvedConflictReport {
        val root = JSONObject(raw)
        check(!root.has("error")) { "conflict_fetch_failed" }
        val rows = root.getJSONArray("conflicts")
        val branches = buildList {
            repeat(rows.length()) { index ->
                val row = rows.getJSONObject(index)
                add(
                    ConflictBranchSummary(
                        snapshotId = row.getString("snapshot_id"),
                        cloudHead = row.getString("cloud_head"),
                        branchDeviceId = row.getString("branch_device_id"),
                        branchCreatedUnixMs = row.getLong("branch_created_unix_ms"),
                        cloudDeviceId = row.getString("cloud_device_id"),
                        cloudCreatedUnixMs = row.getLong("cloud_created_unix_ms"),
                        changedFiles = row.getLong("changed_files"),
                        changedBytes = row.getLong("changed_bytes"),
                    ),
                )
            }
        }
        return UnresolvedConflictReport(
            cloudHead = if (root.isNull("cloud_head")) null else root.getString("cloud_head"),
            branches = branches,
        )
    }
}

class ConflictPipeline(private val context: Context) {
    suspend fun fetch(serverEndpoint: String): UnresolvedConflictReport = withContext(Dispatchers.IO) {
        withSecret { secret ->
            ConflictReportParser.parse(
                NativeSyncBridge.fetchUnresolvedConflicts(
                    SyncServerProbe.normalizeServer(serverEndpoint),
                    secret,
                    SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                    SyncServerProbe.deviceId(context),
                ),
            )
        }
    }

    /** Call only after upload/restore has succeeded and returned [chosenSnapshotId]. */
    suspend fun resolve(
        serverEndpoint: String,
        displayedBranches: List<ConflictBranchSummary>,
        chosenSnapshotId: String,
        replaceWithLocal: Boolean,
    ): ConflictResolutionReport = withContext(Dispatchers.IO) {
        require(chosenSnapshotId.matches(Regex("[0-9a-f]{64}"))) { "invalid_chosen_snapshot" }
        require(displayedBranches.isNotEmpty()) { "no_displayed_conflicts" }
        val ids = JSONArray(displayedBranches.map { it.snapshotId }).toString()
        withSecret { secret ->
            val root = JSONObject(
                NativeSyncBridge.resolveConflicts(
                    SyncServerProbe.normalizeServer(serverEndpoint),
                    secret,
                    SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                    SyncServerProbe.deviceId(context),
                    ids,
                    chosenSnapshotId,
                    replaceWithLocal,
                ),
            )
            check(!root.has("error")) { "conflict_resolve_failed" }
            ConflictResolutionReport(root.getInt("resolved"), root.getInt("total"))
        }
    }

    private inline fun <T> withSecret(block: (ByteArray) -> T): T {
        val secret = AndroidSecretVault(context).load()
        return try { block(secret) } finally { secret.fill(0) }
    }
}
