package org.mhtoolkit.savesync

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class PrelaunchProbeResult(
    val summary: String,
    val reason: String,
    val cloudReachable: Boolean,
    val remoteHead: String?,
)

object SyncServerProbe {
    const val MH3G_NEMESSIX_LOGICAL_SAVE_ID =
        "243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2"

    suspend fun checkPrelaunch(
        serverEndpoint: String,
        emulatorRunning: Boolean,
    ): PrelaunchProbeResult = withContext(Dispatchers.IO) {
        val server = normalizeServer(serverEndpoint)
        if (server.isBlank()) {
            return@withContext PrelaunchProbeResult(
                summary = "还没有服务器地址。可以继续使用本地存档，但不会同步到 Mac；请先填写自部署服务器地址。",
                reason = "prelaunch-no-server",
                cloudReachable = false,
                remoteHead = null,
            )
        }
        try {
            val ready = get("$server/ready")
            if (ready.status !in 200..299) {
                return@withContext cloudUnavailable(server, "ready=${ready.status}")
            }
            val head = get("$server/v1/heads/$MH3G_NEMESSIX_LOGICAL_SAVE_ID")
            when (head.status) {
                in 200..299 -> {
                    val snapshot = head.body.trim().trim('"').ifBlank { "unknown" }
                    PrelaunchProbeResult(
                        summary = if (emulatorRunning) {
                            "云端可用，且 MH3G 有云端版本=$snapshot。Nemessix 正在运行，当前只会下载到缓存；请退出游戏后再执行云端覆盖本地。服务器：$server。"
                        } else {
                            "云端可用，且 MH3G 有云端版本=$snapshot。若本地不是同一版本，请先下载到缓存并确认后恢复；不会按最新时间自动覆盖。服务器：$server。"
                        },
                        reason = "prelaunch-remote-head",
                        cloudReachable = true,
                        remoteHead = snapshot,
                    )
                }
                404 -> PrelaunchProbeResult(
                    summary = "云端可用，但还没有 MH3G 云端版本。可以启动本地游戏；退出后本地稳定快照会上传到 $server。",
                    reason = "prelaunch-no-remote-head",
                    cloudReachable = true,
                    remoteHead = null,
                )
                else -> cloudUnavailable(server, "head=${head.status}")
            }
        } catch (error: IOException) {
            cloudUnavailable(server, error.javaClass.simpleName)
        } catch (error: RuntimeException) {
            cloudUnavailable(server, error.javaClass.simpleName)
        }
    }

    fun normalizeServer(serverEndpoint: String): String =
        serverEndpoint.trim().trimEnd('/')

    private fun cloudUnavailable(server: String, detail: String): PrelaunchProbeResult =
        PrelaunchProbeResult(
            summary = "云端暂时不可用（$detail）。不会破坏本地原始存档；你可以继续使用本地存档游玩，退出后本地快照会保留在队列里，云端恢复后再上传。服务器：$server。",
            reason = "prelaunch-cloud-unavailable",
            cloudReachable = false,
            remoteHead = null,
        )

    private fun get(url: String): HttpResult {
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = "GET"
            connectTimeout = 2_500
            readTimeout = 2_500
        }
        return try {
            val status = connection.responseCode
            val body = if (status in 200..299) {
                connection.inputStream.bufferedReader().use { it.readText() }
            } else {
                connection.errorStream?.bufferedReader()?.use { it.readText() }.orEmpty()
            }
            HttpResult(status, body)
        } finally {
            connection.disconnect()
        }
    }
}

private data class HttpResult(
    val status: Int,
    val body: String,
)
