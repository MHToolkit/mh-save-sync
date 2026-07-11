package org.mhtoolkit.savesync

internal object LocalReplacePolicy {
    fun requireSessionStopped(sessionActive: Boolean) {
        check(!sessionActive) { "请先确认已退出 Nemessix" }
    }

    fun requireObservedBase(observed: String?, current: String?): String? {
        check(observed == current) { "确认期间云端版本已变化，已取消上传；请重新检查" }
        return observed
    }
}

sealed interface LocalReplaceResult {
    data class Uploaded(
        val snapshotId: String,
        val cloudHead: String,
        val fileCount: Int,
        val totalBytes: Long,
    ) : LocalReplaceResult

    data class Conflict(
        val snapshotId: String,
        val cloudHead: String,
        val conflictSnapshot: String,
        val headChanged: Boolean = false,
    ) : LocalReplaceResult

    data object Failed : LocalReplaceResult

    companion object {
        fun parse(json: String): LocalReplaceResult = runCatching {
            if (jsonValue(json, "error") != null) return Failed
            val outcome = requireNotNull(jsonValue(json, "outcome"))
            val snapshot = requireNotNull(jsonValue(json, "snapshot_id"))
            val head = requireNotNull(jsonValue(json, "cloud_head"))
            when (outcome) {
                "conflict" -> Conflict(snapshot, head, requireNotNull(jsonValue(json, "conflict_snapshot")))
                "fast-forward", "created" -> Uploaded(
                    snapshot,
                    head,
                    requireNotNull(jsonNumber(json, "file_count")).toInt(),
                    requireNotNull(jsonNumber(json, "total_bytes")),
                )
                else -> Failed
            }
        }.getOrDefault(Failed)

        private fun jsonValue(json: String, key: String): String? =
            Regex("\\\"${Regex.escape(key)}\\\"\\s*:\\s*\\\"([^\\\"]+)\\\"")
                .find(json)?.groupValues?.get(1)

        private fun jsonNumber(json: String, key: String): Long? =
            Regex("\\\"${Regex.escape(key)}\\\"\\s*:\\s*(\\d+)")
                .find(json)?.groupValues?.get(1)?.toLongOrNull()
    }
}
