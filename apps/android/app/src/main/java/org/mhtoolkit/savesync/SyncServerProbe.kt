package org.mhtoolkit.savesync

import android.content.Context
import android.provider.Settings
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

data class PrelaunchProbeResult(
    val summary: String,
    val reason: String,
    val cloudReachable: Boolean,
    val remoteHead: String?,
    val remoteVersionLabel: String? = null,
)

object SyncServerProbe {
    const val MH3G_NEMESSIX_LOGICAL_SAVE_ID =
        "243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2"

    suspend fun checkPrelaunch(
        context: Context,
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
        if (!AndroidSecretVault(context).hasSecret()) {
            return@withContext PrelaunchProbeResult(
                summary = "请先导入与 Mac 相同的恢复密钥，才能验证云端版本。此时不会读取或修改本地存档。",
                reason = "prelaunch-key-required",
                cloudReachable = false,
                remoteHead = null,
            )
        }
        try {
            val ready = get("$server/ready")
            if (ready.status !in 200..299) {
                return@withContext cloudUnavailable(server, "ready=${ready.status}")
            }
            val snapshot = fetchHeadForReplace(context, server)
            when {
                snapshot != null -> {
                    val versionLabel = userVisibleRemoteVersion(snapshot)
                    PrelaunchProbeResult(
                        summary = if (emulatorRunning) {
                            "云端可用，且 $versionLabel。Nemessix 正在运行，当前只会下载到本机缓存；请退出游戏后再执行云端覆盖本地。服务器：$server。"
                        } else {
                            "云端可用，且 $versionLabel。若本地不是同一版本，请先下载到本机缓存并确认后恢复；不会按最新时间自动覆盖。服务器：$server。"
                        },
                        reason = "prelaunch-remote-head",
                        cloudReachable = true,
                        remoteHead = snapshot.ifBlank { null },
                        remoteVersionLabel = versionLabel,
                    )
                }
                else -> PrelaunchProbeResult(
                    summary = "云端可用，但还没有 MH3G 云端版本。可以启动本地游戏；退出后本地稳定快照会上传到 $server。",
                    reason = "prelaunch-no-remote-head",
                    cloudReachable = true,
                    remoteHead = null,
                )
            }
        } catch (error: IOException) {
            cloudUnavailable(server, error.javaClass.simpleName)
        } catch (error: RuntimeException) {
            cloudUnavailable(server, error.javaClass.simpleName)
        }
    }

    /** Returns the exact CAS base. Network/protocol failures are never mapped to an empty head. */
    suspend fun fetchHeadForReplace(
        context: Context,
        serverEndpoint: String,
    ): String? = withContext(Dispatchers.IO) {
        val server = normalizeServer(serverEndpoint)
        require(server.isNotBlank()) { "请先填写服务器地址" }
        val ready = get("$server/ready")
        check(ready.status in 200..299) { "服务器暂时不可用" }
        var secret: ByteArray? = null
        try {
            secret = AndroidSecretVault(context).load()
            val response = JSONObject(
                NativeSyncBridge.fetchCloudHead(
                    serverEndpoint = server,
                    recoverySecret = secret,
                    logicalSaveId = MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                    deviceId = deviceId(context),
                ),
            )
            check(!response.has("error")) { "无法验证云端版本" }
            if (response.isNull("head")) null else response.getString("head").ifBlank { null }
        } finally {
            secret?.fill(0)
        }
    }

    private fun deviceId(context: Context): String {
        val raw = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID)
            .orEmpty().toByteArray()
        return try {
            "android-" + MessageDigest.getInstance("SHA-256").digest(raw)
                .take(8).joinToString("") { "%02x".format(it) }
        } finally {
            raw.fill(0)
        }
    }

    fun normalizeServer(serverEndpoint: String): String =
        serverEndpoint.trim().trimEnd('/')

    fun userVisibleRemoteVersion(rawSnapshot: String): String {
        val trimmed = rawSnapshot.trim().trim('"')
        if (trimmed.isBlank()) {
            return "MH3G 云端已有一个版本，详情暂不可读"
        }
        val suffix = trimmed.takeLast(minOf(6, trimmed.length))
        return "MH3G 云端已有一个版本（版本摘要后 6 位：$suffix）"
    }

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
