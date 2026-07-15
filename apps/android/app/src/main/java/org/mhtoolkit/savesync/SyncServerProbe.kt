package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
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
    val state: PrelaunchConsistencyState,
)

object SyncServerProbe {
    const val MH3G_NEMESSIX_LOGICAL_SAVE_ID =
        "243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2"

    suspend fun checkPrelaunch(
        context: Context,
        serverEndpoint: String,
        emulatorRunning: Boolean,
        treeUri: String?,
    ): PrelaunchProbeResult = withContext(Dispatchers.IO) {
        val server = normalizeServer(serverEndpoint)
        if (server.isBlank()) {
            return@withContext PrelaunchProbeResult(
                summary = "还没有服务器地址。可以继续使用本地存档，但不会同步到 Mac；请先填写自部署服务器地址。",
                reason = "prelaunch-no-server",
                cloudReachable = false,
                remoteHead = null,
                state = PrelaunchConsistencyState.NO_SERVER,
            )
        }
        if (!AndroidSecretVault(context).hasSecret()) {
            return@withContext PrelaunchProbeResult(
                summary = "请先导入与 Mac 相同的恢复密钥，才能验证云端版本。此时不会读取或修改本地存档。",
                reason = "prelaunch-key-required",
                cloudReachable = false,
                remoteHead = null,
                state = PrelaunchConsistencyState.KEY_REQUIRED,
            )
        }
        try {
            val ready = get("$server/ready")
            if (ready.status !in 200..299) {
                return@withContext cloudUnavailable(server, "ready=${ready.status}")
            }
            val snapshot = fetchHeadForReplace(context, server)
            if (!PrelaunchCapturePolicy.shouldCaptureLocal(emulatorRunning)) {
                val state = PrelaunchConsistencyState.EMULATOR_RUNNING
                return@withContext PrelaunchProbeResult(
                    summary = summaryFor(state),
                    reason = state.reason,
                    cloudReachable = true,
                    remoteHead = snapshot,
                    remoteVersionLabel = snapshot?.let(::userVisibleRemoteVersion),
                    state = state,
                )
            }
            if (treeUri.isNullOrBlank()) {
                return@withContext localUnavailable(snapshot, "尚未授权 Nemessix 存档目录")
            }
            val observation = try {
                PrelaunchObservationCoordinator.captureThenRefetch(
                    captureLocal = {
                        SafStableStager(context).capture(Uri.parse(treeUri)).let { stage ->
                            try { stage.fingerprint } finally { stage.root.deleteRecursively() }
                        }
                    },
                    refetchRemoteHead = { fetchHeadForReplace(context, server) },
                )
            } catch (_: LocalCaptureUnavailableException) {
                return@withContext localUnavailable(snapshot, "无法稳定读取已授权的存档目录")
            }
            val binding = SyncConsistencyBinding(
                serverEndpoint = server,
                logicalSaveId = MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                treeUri = treeUri,
                deviceId = deviceId(context),
            )
            val state = PrelaunchConsistencyPolicy.classify(
                binding = binding,
                baseline = SyncConsistencyLedgerStore(context).read(),
                localFingerprint = LocalFingerprintObservation.Available(observation.localFingerprint),
                remoteHead = observation.remoteHead,
                emulatorRunning = emulatorRunning,
            )
            val versionLabel = observation.remoteHead?.let(::userVisibleRemoteVersion)
            PrelaunchProbeResult(
                summary = summaryFor(state),
                reason = state.reason,
                cloudReachable = true,
                remoteHead = observation.remoteHead,
                remoteVersionLabel = versionLabel,
                state = state,
            )
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

    internal fun deviceId(context: Context): String {
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
            state = PrelaunchConsistencyState.CLOUD_UNAVAILABLE,
        )

    private fun localUnavailable(remoteHead: String?, detail: String) = PrelaunchProbeResult(
        summary = "$detail。未读取出可信指纹，因此不会判断手机与云端是否一致；可明确选择仅使用本地存档启动。",
        reason = PrelaunchConsistencyState.LOCAL_UNAVAILABLE.reason,
        cloudReachable = true,
        remoteHead = remoteHead,
        remoteVersionLabel = remoteHead?.let(::userVisibleRemoteVersion),
        state = PrelaunchConsistencyState.LOCAL_UNAVAILABLE,
    )

    private fun summaryFor(state: PrelaunchConsistencyState): String = when (state) {
        PrelaunchConsistencyState.SYNCED -> "手机存档与已验证的云端版本一致，可以直接启动 Nemessix。"
        PrelaunchConsistencyState.REMOTE_ADVANCED -> "手机存档未变，但云端已有其他设备的新进度；建议恢复云端版本后再启动。"
        PrelaunchConsistencyState.LOCAL_CHANGED -> "云端版本未变，但手机存档有新进度；建议先上传手机存档。"
        PrelaunchConsistencyState.DIVERGED -> "手机与云端都从上次确认的版本继续产生了新进度，请选择保留方向；不会自动覆盖。"
        PrelaunchConsistencyState.UNKNOWN -> "这是当前服务器、账号目录或设备的首次可信检查，无法证明两边相同；请选择使用手机或云端版本。"
        PrelaunchConsistencyState.NO_REMOTE -> "云端还没有此游戏的存档，可以启动 Nemessix；退出后再上传稳定快照。"
        PrelaunchConsistencyState.EMULATOR_RUNNING -> "Nemessix 正在运行；不会恢复或覆盖正在使用的存档。"
        else -> "启动前检查尚未建立可信结论。"
    }

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
