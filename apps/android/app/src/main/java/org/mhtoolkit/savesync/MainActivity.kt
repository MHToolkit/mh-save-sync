package org.mhtoolkit.savesync

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ApplicationInfo
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (
            Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 10)
        }
        SyncScheduler.ensureDefaults(this)
        val restoreRoot = java.io.File(noBackupFilesDir, "restore")
        RestoreRecovery.cleanupNonMutating(this, restoreRoot)
        getSharedPreferences(SyncScheduler.PREFERENCES, MODE_PRIVATE).edit()
            .putString(
                SyncScheduler.NATIVE_BRIDGE_HEALTH,
                runCatching { NativeSyncBridge.bridgeHealth() }.getOrElse { "unavailable:${it.javaClass.simpleName}" },
            )
            .putInt(
                SyncScheduler.PENDING_RESTORE_RECOVERY_COUNT,
                RestoreRecovery.pending(restoreRoot).size,
            ).apply()
        SyncScheduler.ensurePeriodic(this)
        if (
            applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0 &&
            intent.getBooleanExtra("probe_quiescence", false)
        ) {
            Thread {
                var lease: NemessixQuiescenceLease? = null
                var released = false
                val result = runCatching {
                    val client = NemessixQuiescenceClient(this)
                    lease = client.acquire()
                    client.validate(requireNotNull(lease))
                    client.release(requireNotNull(lease), "ABORTED", requireNotNull(lease).challengeHex)
                    released = true
                    "available:${requireNotNull(lease).emulatorBuild}"
                }.getOrElse {
                    val origin = it.stackTrace.firstOrNull()
                    "unavailable:${it.javaClass.simpleName}:${origin?.className?.substringAfterLast('.')}:${origin?.methodName}"
                }.also {
                    if (!released) {
                        lease?.let { held ->
                            runCatching {
                                NemessixQuiescenceClient(this).release(held, "ABORTED", held.challengeHex)
                            }
                        }
                    }
                }
                getSharedPreferences(SyncScheduler.PREFERENCES, MODE_PRIVATE).edit()
                    .putString("nemessix_quiescence_probe", result)
                    .apply()
            }.start()
        }
        setContent {
            MaterialTheme {
                SaveSyncDashboard()
            }
        }
    }

    @Composable
    private fun SaveSyncDashboard() {
        val preferences = remember {
            getSharedPreferences(SyncScheduler.PREFERENCES, MODE_PRIVATE)
        }
        var authorized by remember {
            mutableStateOf(preferences.contains(SyncScheduler.SAF_ROOT))
        }
        var wifiOnly by remember {
            mutableStateOf(preferences.getBoolean(SyncScheduler.WIFI_ONLY, true))
        }
        var sessionActive by remember {
            mutableStateOf(preferences.getBoolean(SyncScheduler.SESSION_ACTIVE, true))
        }
        var gameEnabled by remember {
            mutableStateOf(preferences.getBoolean(SyncScheduler.GAME_MH3G_ENABLED, true))
        }
        var serverEndpoint by remember {
            mutableStateOf(preferences.getString(SyncScheduler.SERVER_ENDPOINT, null).orEmpty())
        }
        var lastSummary by remember {
            mutableStateOf(
                preferences.getString(
                    SyncScheduler.LAST_SYNC_SUMMARY,
                    "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。",
                ).orEmpty()
            )
        }
        var syncPhase by remember {
            mutableStateOf(preferences.getString(SyncScheduler.LAST_SYNC_PHASE, "暂无后台任务").orEmpty())
        }
        var nextAction by remember {
            mutableStateOf(
                preferences.getString(
                    SyncScheduler.LAST_SYNC_NEXT_ACTION,
                    "先填写服务器地址并授权存档目录，然后做启动前检查。",
                ).orEmpty()
            )
        }
        var syncError by remember {
            mutableStateOf(preferences.getString(SyncScheduler.LAST_SYNC_ERROR, "").orEmpty())
        }
        var launchGate by remember {
            mutableStateOf(DashboardContentPolicy.launchStatus(
                preferences.getString(SyncScheduler.LAUNCH_GATE_REASON, "not-checked").orEmpty()
            ))
        }
        var launchGateReason by remember {
            mutableStateOf(
                preferences.getString(
                    SyncScheduler.LAUNCH_GATE_REASON,
                    "not-checked",
                ).orEmpty()
            )
        }
        var conflictVisible by remember { mutableStateOf(false) }
        var helpVisible by remember { mutableStateOf(false) }
        var settingsVisible by remember { mutableStateOf(false) }
        var restoreCloudConfirmVisible by remember { mutableStateOf(false) }
        var localReplaceCloudConfirmVisible by remember { mutableStateOf(false) }
        var recoverySecretVisible by remember { mutableStateOf(false) }
        var recoverySecretInput by remember { mutableStateOf("") }
        var recoverySecretError by remember { mutableStateOf("") }
        var hasRecoverySecret by remember { mutableStateOf(AndroidSecretVault(this).hasSecret()) }
        var observedReplaceHead by remember { mutableStateOf<String?>(null) }
        var replaceProbeInProgress by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        val serverEndpointFocusRequester = remember { FocusRequester() }
        val keyboardController = LocalSoftwareKeyboardController.current

        fun refreshDashboardStateFromPreferences() {
            authorized = preferences.contains(SyncScheduler.SAF_ROOT)
            wifiOnly = preferences.getBoolean(SyncScheduler.WIFI_ONLY, true)
            sessionActive = preferences.getBoolean(SyncScheduler.SESSION_ACTIVE, true)
            gameEnabled = preferences.getBoolean(SyncScheduler.GAME_MH3G_ENABLED, true)
            serverEndpoint = preferences.getString(SyncScheduler.SERVER_ENDPOINT, null).orEmpty()
            lastSummary = preferences.getString(
                SyncScheduler.LAST_SYNC_SUMMARY,
                "还没有同步记录。先填写服务器地址并授权 Android Nemessix 存档目录。",
            ).orEmpty()
            syncPhase = preferences.getString(SyncScheduler.LAST_SYNC_PHASE, "暂无后台任务").orEmpty()
            nextAction = preferences.getString(
                SyncScheduler.LAST_SYNC_NEXT_ACTION,
                "先填写服务器地址并授权存档目录，然后做启动前检查。",
            ).orEmpty()
            syncError = preferences.getString(SyncScheduler.LAST_SYNC_ERROR, "").orEmpty()
            launchGateReason = preferences.getString(
                SyncScheduler.LAUNCH_GATE_REASON,
                "not-checked",
            ).orEmpty()
            launchGate = DashboardContentPolicy.launchStatus(launchGateReason)
        }

        DisposableEffect(preferences) {
            val listener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
                if (
                    key in setOf(
                        SyncScheduler.SAF_ROOT,
                        SyncScheduler.WIFI_ONLY,
                        SyncScheduler.SERVER_ENDPOINT,
                        SyncScheduler.LAST_SYNC_SUMMARY,
                        SyncScheduler.LAST_SYNC_PHASE,
                        SyncScheduler.LAST_SYNC_NEXT_ACTION,
                        SyncScheduler.LAST_SYNC_ERROR,
                        SyncScheduler.LAUNCH_GATE_SUMMARY,
                        SyncScheduler.LAUNCH_GATE_REASON,
                        SyncScheduler.SESSION_ACTIVE,
                        SyncScheduler.GAME_MH3G_ENABLED,
                    )
                ) {
                    refreshDashboardStateFromPreferences()
                }
            }
            preferences.registerOnSharedPreferenceChangeListener(listener)
            refreshDashboardStateFromPreferences()
            onDispose {
                preferences.unregisterOnSharedPreferenceChangeListener(listener)
            }
        }

        DisposableEffect(recoverySecretVisible) {
            if (recoverySecretVisible) {
                window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
            onDispose {
                window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
        }

        val folderPicker = rememberLauncherForActivityResult(
            ActivityResultContracts.OpenDocumentTree(),
        ) { uri ->
            if (uri != null) {
                val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                contentResolver.takePersistableUriPermission(uri, flags)
                preferences.edit().putString(SyncScheduler.SAF_ROOT, uri.toString()).apply()
                authorized = true
                lastSummary = "已授权 Android Nemessix 存档目录。不会立刻上传；只有稳定快照通过校验后才会进入上传队列。"
                preferences.edit()
                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                    .putString(SyncScheduler.LAST_SYNC_TARGET, "MH3G / Android Nemessix")
                    .apply()
            }
        }

        fun persistSyncStatus(
            reason: String,
            summary: String,
            phase: String,
            action: String,
            error: String = "",
        ) {
            lastSummary = summary
            syncPhase = phase
            nextAction = action
            syncError = error
            preferences.edit()
                .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                .putString(SyncScheduler.LAST_SYNC_REASON, reason)
                .putString(SyncScheduler.LAST_SYNC_PHASE, syncPhase)
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, nextAction)
                .putString(SyncScheduler.LAST_SYNC_ERROR, syncError)
                .apply()
        }

        fun persistNoServerStatus(reason: String, actionLabel: String) {
            persistSyncStatus(
                reason = reason,
                summary = SyncMessages.cloudActionNeedsServer(),
                phase = SyncMessages.noServerPhase(),
                action = SyncMessages.noServerNextAction(actionLabel),
                error = SyncMessages.noServerError(),
            )
        }

        fun executeLocalReplaceCloud() {
            val observed = observedReplaceHead
            persistSyncStatus(
                reason = "user-use-local",
                summary = "正在读取两次稳定存档并加密上传；不会修改本地原始存档。",
                phase = "正在验证并上传",
                action = "请保持应用在前台；失败不会伪报成功。",
            )
            scope.launch {
                val tree = preferences.getString(SyncScheduler.SAF_ROOT, null)
                val result = runCatching {
                    requireNotNull(tree) { "请先授权 Nemessix 存档目录" }
                    LocalReplacePipeline(this@MainActivity).execute(
                        serverEndpoint,
                        android.net.Uri.parse(tree),
                        observed,
                        sessionActive,
                    )
                }
                result.fold(
                    onSuccess = { upload ->
                        when (upload) {
                            is LocalReplaceResult.Uploaded -> persistSyncStatus(
                                "user-use-local",
                                "本地存档已设为云端最新（版本 …${upload.cloudHead.takeLast(6)}，${upload.fileCount} 个文件）。",
                                "上传完成",
                                "Mac 端启动前检查后即可看到该版本。",
                            )
                            is LocalReplaceResult.Conflict -> persistSyncStatus(
                                "user-use-local-conflict",
                                "确认后云端版本仍发生竞争变化，本地快照已保留为冲突分支 …${upload.snapshotId.takeLast(6)}，当前云端版本仍为 …${upload.cloudHead.takeLast(6)}。",
                                "已保留冲突，未覆盖云端",
                                "重新检查云端版本后再决定，不会静默覆盖。",
                            )
                            LocalReplaceResult.Failed -> persistSyncStatus(
                                "user-use-local-failed", "上传失败；本地原始存档和当前云端版本均未被本应用声称修改。",
                                "上传失败", "检查网络、密钥和目录授权后重试。", "同步失败",
                            )
                        }
                    },
                    onFailure = {
                        persistSyncStatus(
                            "user-use-local-failed", "本地替换云端未完成；不会伪报成功。",
                            "上传失败", "确认 Nemessix 已退出、网络可用后重新检查。",
                            "同步失败（代码：upload_failed）",
                        )
                    },
                )
            }
        }

        fun queueRestoreCloudHead() {
            val reason = "restore-cloud-head"
            persistSyncStatus(
                reason = reason,
                summary = "正在验证云端版本并创建恢复前备份。",
                phase = "正在安全恢复",
                action = "请保持 Nemessix 关闭；失败会自动回滚。",
            )
            scope.launch {
                runCatching {
                    val tree = preferences.getString(SyncScheduler.SAF_ROOT, null)
                        ?: error("请先授权 Nemessix 存档目录")
                    CloudRestorePipeline(this@MainActivity).execute(
                        serverEndpoint,
                        android.net.Uri.parse(tree),
                        sessionActive,
                        RestoreStopEvidence.confirmed(sessionActive),
                    )
                }.onSuccess { restored ->
                    persistSyncStatus(
                        reason, "已从云端恢复版本 …${restored.snapshotId.takeLast(6)}（${restored.fileCount} 个文件），恢复前备份已保留。",
                        "恢复完成", "现在可以启动 Nemessix 检查存档。",
                    )
                }.onFailure {
                    persistSyncStatus(
                        "restore-cloud-head-failed", "云端恢复未完成；未静默覆盖，失败时已尝试回滚。",
                        "恢复失败", "保持 Nemessix 关闭并重试；恢复前备份和日志仍保留。",
                        "恢复失败（代码：restore_failed）",
                    )
                }
            }
        }

        fun downloadCloudHeadToCache() {
            persistSyncStatus(
                "download-cache-only", "正在下载并验证云端版本；不会修改 Nemessix 存档。",
                "正在下载", "完成后仍需等待安全停止证明才能恢复。",
            )
            scope.launch {
                runCatching { CloudDownloadPipeline(this@MainActivity).execute(serverEndpoint) }
                    .onSuccess { cached -> persistSyncStatus(
                        "download-cache-only", "云端版本 …${cached.snapshotId.takeLast(6)} 已加密保存到本机。",
                        "下载完成", "当前不会覆盖 Nemessix 存档。",
                    ) }
                    .onFailure { persistSyncStatus(
                        "download-cache-failed", "云端版本下载或校验失败；本地存档未修改。",
                        "下载失败", "检查网络、服务器和恢复密钥后重试。", "下载失败（代码：cloud_cache_failed）",
                    ) }
            }
        }

        fun runPrelaunchCheck() {
            launchGate = "正在检查 ${SyncMessages.serverLabel(serverEndpoint)} 是否可用，并查看 MH3G 是否有云端版本；不会修改本地存档。"
            preferences.edit()
                .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                .putString(SyncScheduler.LAUNCH_GATE_REASON, "prelaunch-checking")
                .putString(SyncScheduler.LAST_SYNC_REASON, "prelaunch-checking")
                .apply()
            scope.launch {
                val result = SyncServerProbe.checkPrelaunch(
                    context = this@MainActivity,
                    serverEndpoint = serverEndpoint,
                    emulatorRunning = sessionActive,
                )
                launchGateReason = result.reason
                launchGate = DashboardContentPolicy.launchStatus(launchGateReason)
                lastSummary = result.summary
                preferences.edit()
                    .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                    .putString(SyncScheduler.LAUNCH_GATE_REASON, launchGateReason)
                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                    .putString(SyncScheduler.LAST_SYNC_REASON, result.reason)
                    .putString(SyncScheduler.REMOTE_VERSION_LABEL, result.remoteVersionLabel.orEmpty())
                    .apply()
            }
        }

        fun toggleSessionProtection() {
            if (sessionActive) {
                stopService(Intent(this@MainActivity, ActiveSessionService::class.java))
                lastSummary = SyncMessages.sessionExitSummary()
            } else {
                ContextCompat.startForegroundService(
                    this@MainActivity,
                    Intent(this@MainActivity, ActiveSessionService::class.java),
                )
                lastSummary = SyncMessages.sessionStartSummary()
            }
            val oldSessionActive = sessionActive
            syncPhase = if (oldSessionActive) SyncMessages.queuedPhase("session-exit") else "游戏运行保护中"
            nextAction = if (oldSessionActive) SyncMessages.queuedNextAction("session-exit", false) else "游玩期间不会把云端覆盖到本地；退出后再对账上传。"
            syncError = ""
            sessionActive = !sessionActive
            preferences.edit()
                .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                .putString(
                    SyncScheduler.LAST_SYNC_REASON,
                    if (sessionActive) "session-start" else "session-exit",
                )
                .putBoolean(SyncScheduler.SESSION_ACTIVE, sessionActive)
                .putString(SyncScheduler.LAST_SYNC_PHASE, syncPhase)
                .putString(SyncScheduler.LAST_SYNC_NEXT_ACTION, nextAction)
                .putString(SyncScheduler.LAST_SYNC_ERROR, syncError)
                .apply()
        }

        if (conflictVisible) {
            ConflictDialog(
                onDismiss = { conflictVisible = false },
            )
        }

        if (restoreCloudConfirmVisible) {
            RestoreCloudConfirmDialog(
                serverEndpoint = serverEndpoint,
                onDismiss = { restoreCloudConfirmVisible = false },
                onConfirm = {
                    restoreCloudConfirmVisible = false
                    if (sessionActive) {
                        persistSyncStatus(
                            reason = "restore-blocked-running",
                            summary = SyncMessages.restoreBlockedRunning(),
                            phase = SyncMessages.completedPhase("restore-blocked-running"),
                            action = SyncMessages.completedNextAction("restore-blocked-running", sessionActive),
                            error = "Nemessix 仍在运行",
                        )
                    } else {
                        queueRestoreCloudHead()
                    }
                },
            )
        }

        if (localReplaceCloudConfirmVisible) {
            LocalReplaceCloudConfirmDialog(
                serverEndpoint = serverEndpoint,
                sessionActive = sessionActive,
                observedHead = observedReplaceHead,
                onDismiss = { localReplaceCloudConfirmVisible = false },
                onConfirm = {
                    localReplaceCloudConfirmVisible = false
                    executeLocalReplaceCloud()
                },
            )
        }

        if (recoverySecretVisible) {
            AlertDialog(
                onDismissRequest = {
                    recoverySecretInput = ""
                    recoverySecretError = ""
                    recoverySecretVisible = false
                },
                title = { Text("导入恢复密钥") },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("一期仅接受 64 位十六进制恢复密钥。密钥会由 Android Keystore 加密保存，不会发送到服务器。")
                        OutlinedTextField(
                            value = recoverySecretInput,
                            onValueChange = { recoverySecretInput = it; recoverySecretError = "" },
                            label = { Text("64 位十六进制密钥") },
                            isError = recoverySecretError.isNotBlank(),
                            supportingText = { if (recoverySecretError.isNotBlank()) Text(recoverySecretError) },
                            visualTransformation = PasswordVisualTransformation(),
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                },
                confirmButton = {
                    TextButton(onClick = {
                        var decoded: ByteArray? = null
                        try {
                            decoded = RecoverySecretFormat.decodeHex(recoverySecretInput)
                            recoverySecretInput = ""
                            AndroidSecretVault(this@MainActivity).store(decoded)
                            hasRecoverySecret = true
                            recoverySecretVisible = false
                            recoverySecretError = ""
                        } catch (error: IllegalArgumentException) {
                            recoverySecretError = "恢复密钥格式不正确"
                        } finally {
                            decoded?.fill(0)
                        }
                    }) { Text("安全导入") }
                },
                dismissButton = {
                    TextButton(onClick = {
                        recoverySecretInput = ""
                        recoverySecretVisible = false
                    }) { Text("取消") }
                },
            )
        }

        if (helpVisible) {
            HelpDialog(
                onConflictHelp = {
                    helpVisible = false
                    conflictVisible = true
                },
                onDismiss = { helpVisible = false },
            )
        }

        Scaffold { padding ->
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .verticalScroll(rememberScrollState())
                    .padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text("MH 云存档", style = MaterialTheme.typography.headlineMedium)
                        Text("MH3G · Android Nemessix", style = MaterialTheme.typography.bodyMedium)
                    }
                    TextButton(onClick = { helpVisible = true }) {
                        Text(DashboardContentPolicy.helpLabel)
                    }
                }

                CardSection("存档状态") {
                    Text(
                        DashboardContentPolicy.status(
                            authorized,
                            gameEnabled,
                            serverEndpoint.isNotBlank(),
                            sessionActive,
                        ),
                        style = MaterialTheme.typography.titleMedium,
                    )
                    Text(syncPhase, color = MaterialTheme.colorScheme.primary)
                    if (syncError.isNotBlank()) {
                        Text(syncError, color = MaterialTheme.colorScheme.error)
                    }
                }

                CardSection("快速同步") {
                    Button(
                        enabled = authorized && gameEnabled && hasRecoverySecret &&
                            SyncScheduler.LOCAL_REPLACE_PIPELINE_AVAILABLE && !replaceProbeInProgress,
                        onClick = {
                            if (serverEndpoint.isBlank()) {
                                persistNoServerStatus("user-use-local-no-server", "上传手机存档")
                            } else {
                                replaceProbeInProgress = true
                                persistSyncStatus(
                                    "user-use-local-probe", "正在检查云端版本。",
                                    "检查云端", "检查完成后请确认上传。",
                                )
                                scope.launch {
                                    runCatching {
                                        LocalReplacePolicy.requireSessionStopped(sessionActive)
                                        NemessixProcessGate(this@MainActivity).requireStopped()
                                        SyncServerProbe.fetchHeadForReplace(this@MainActivity, serverEndpoint)
                                    }.fold(
                                        onSuccess = {
                                            observedReplaceHead = it
                                            localReplaceCloudConfirmVisible = true
                                            persistSyncStatus(
                                                "user-use-local-confirm", "云端版本已检查。",
                                                "等待确认", "确认后上传稳定快照。",
                                            )
                                        },
                                        onFailure = {
                                            persistSyncStatus(
                                                "user-use-local-probe-failed", "无法开始上传。",
                                                "检查失败", "请退出 Nemessix 并检查网络。",
                                                "检查失败（代码：cloud_probe_failed）",
                                            )
                                        },
                                    )
                                    replaceProbeInProgress = false
                                }
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(DashboardContentPolicy.uploadLabel) }
                    OutlinedButton(
                        enabled = authorized && gameEnabled && hasRecoverySecret &&
                            SyncScheduler.CLOUD_RESTORE_PIPELINE_AVAILABLE,
                        onClick = {
                            if (serverEndpoint.isBlank()) {
                                persistNoServerStatus("restore-no-server", "恢复云端存档")
                            } else {
                                restoreCloudConfirmVisible = true
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(DashboardContentPolicy.restoreLabel) }
                    Text("上传保留旧版本；恢复前自动备份本地。", style = MaterialTheme.typography.bodySmall)
                }

                CardSection("启动游戏") {
                    Button(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            launchGate = "正在检查云端…"
                            scope.launch {
                                val result = SyncServerProbe.checkPrelaunch(
                                    context = this@MainActivity,
                                    serverEndpoint = serverEndpoint,
                                    emulatorRunning = sessionActive,
                                )
                                launchGateReason = result.reason
                                launchGate = DashboardContentPolicy.launchStatus(launchGateReason)
                                lastSummary = if (result.remoteHead != null) {
                                    "发现云端版本，请先选择上传或恢复。"
                                } else if (!result.cloudReachable) {
                                    SyncMessages.launchPausedForCloudUnavailable()
                                } else {
                                    launchNemessixOrExplain()
                                }
                                preferences.edit()
                                    .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                                    .putString(SyncScheduler.LAUNCH_GATE_REASON, launchGateReason)
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .apply()
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(DashboardContentPolicy.launchLabel) }
                    Text(launchGate, style = MaterialTheme.typography.bodySmall)
                    val canContinueLocal = launchGateReason in setOf(
                        "prelaunch-no-server", "prelaunch-cloud-unavailable", "prelaunch-remote-head",
                    )
                    if (canContinueLocal) {
                        OutlinedButton(
                            onClick = { launchNemessixOrExplain() },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text("仍使用本地存档启动") }
                    }
                }

                CardSection("最近记录") {
                    StatusLine("最近同步", lastSummary)
                    StatusLine("下一步", nextAction)
                    Text(formatTime(preferences.getLong(SyncScheduler.LAST_SYNC_UNIX_MS, 0L)),
                        style = MaterialTheme.typography.labelMedium)
                }

                OutlinedButton(
                    onClick = { settingsVisible = !settingsVisible },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(if (settingsVisible) "收起设置" else DashboardContentPolicy.settingsLabel)
                }

                if (settingsVisible) {
                    CardSection("连接与游戏") {
                        OutlinedTextField(
                            value = serverEndpoint,
                            onValueChange = {
                                serverEndpoint = it
                                preferences.edit().putString(SyncScheduler.SERVER_ENDPOINT, it.trim()).apply()
                            },
                            label = { Text("服务器地址") },
                            placeholder = { Text("http://服务器:端口") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth().focusRequester(serverEndpointFocusRequester),
                        )
                        SettingSwitch("同步 MH3G", gameEnabled) {
                            gameEnabled = it
                            preferences.edit().putBoolean(SyncScheduler.GAME_MH3G_ENABLED, it).apply()
                        }
                    }
                    CardSection("存档与密钥") {
                        StatusLine("存档目录", if (authorized) "已授权" else "未授权")
                        OutlinedButton(onClick = { folderPicker.launch(null) }, modifier = Modifier.fillMaxWidth()) {
                            Text(if (authorized) "更换存档目录" else "选择存档目录")
                        }
                        StatusLine("恢复密钥", if (hasRecoverySecret) "已导入" else "未导入")
                        OutlinedButton(
                            onClick = { recoverySecretVisible = true },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text(if (hasRecoverySecret) "重新导入密钥" else "导入密钥") }
                    }
                    CardSection("同步偏好") {
                        SettingSwitch("仅 Wi-Fi 上传", wifiOnly) {
                            wifiOnly = it
                            preferences.edit().putBoolean(SyncScheduler.WIFI_ONLY, it).apply()
                            SyncScheduler.ensurePeriodic(this@MainActivity)
                        }
                        OutlinedButton(
                            enabled = authorized && gameEnabled && hasRecoverySecret &&
                                SyncScheduler.CLOUD_DOWNLOAD_PIPELINE_AVAILABLE,
                            onClick = {
                                if (serverEndpoint.isBlank()) persistNoServerStatus("download-cache-no-server", "下载到缓存")
                                else downloadCloudHeadToCache()
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text("仅下载到安全缓存") }
                        OutlinedButton(
                            enabled = authorized && gameEnabled,
                            onClick = { toggleSessionProtection() },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text(SyncMessages.activeSessionToggleLabel(sessionActive)) }
                    }
                }

                Spacer(Modifier.height(8.dp))
            }
        }
    }

    @Composable
    private fun CardSection(title: String, content: @Composable ColumnScope.() -> Unit) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                HorizontalDivider()
                content()
            }
        }
    }

    @Composable
    private fun StatusLine(label: String, value: String) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label, style = MaterialTheme.typography.labelLarge)
            Text(value, style = MaterialTheme.typography.bodyMedium)
        }
    }

    @Composable
    private fun SettingSwitch(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(label, modifier = Modifier.weight(1f), fontWeight = FontWeight.Medium)
            Switch(checked = checked, onCheckedChange = onCheckedChange)
        }
    }

    @Composable
    private fun HelpDialog(onConflictHelp: () -> Unit, onDismiss: () -> Unit) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text("使用帮助") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text("第一次怎么用", fontWeight = FontWeight.Bold)
                    Text("在设置中填写服务器、授权存档目录并导入密钥。回家先恢复云端；游玩结束后上传手机存档。")
                    Text("同步安全吗", fontWeight = FontWeight.Bold)
                    Text("服务器只保存加密快照。恢复前会备份本地，旧版本不会立即删除。")
                    TextButton(onClick = onConflictHelp) { Text("冲突怎么处理") }
                }
            },
            confirmButton = { TextButton(onClick = onDismiss) { Text("知道了") } },
        )
    }

    @Composable
    private fun ConflictDialog(
        onDismiss: () -> Unit,
    ) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text("冲突处理说明") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("这是说明页，不会执行覆盖或上传。真正发生冲突时，App 会列出本地与云端的设备、时间、上一个版本、大小和校验摘要。")
                    Text(SyncMessages.conflictDiffBoundary())
                    Text("不会按最新时间自动覆盖。你可以回到「同步动作」选择云端覆盖本地、本地替换云端，或暂不处理；另一边会保留为历史/冲突分支。")
                    Text("二进制游戏存档不做语义合并；只能选择一方或复制为分支。")
                }
            },
            confirmButton = {
                TextButton(onClick = onDismiss) {
                    Text("知道了")
                }
            },
        )
    }

    @Composable
    private fun RestoreCloudConfirmDialog(
        serverEndpoint: String,
        onDismiss: () -> Unit,
        onConfirm: () -> Unit,
    ) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(SyncMessages.restoreCloudConfirmTitle()) },
            text = { Text(SyncMessages.restoreCloudConfirmBody(serverEndpoint)) },
            confirmButton = {
                TextButton(onClick = onConfirm) {
                    Text("确认云端覆盖本地")
                }
            },
            dismissButton = {
                TextButton(onClick = onDismiss) {
                    Text("先继续使用本地")
                }
            },
        )
    }

    @Composable
    private fun LocalReplaceCloudConfirmDialog(
        serverEndpoint: String,
        sessionActive: Boolean,
        observedHead: String?,
        onDismiss: () -> Unit,
        onConfirm: () -> Unit,
    ) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(SyncMessages.localReplaceCloudConfirmTitle()) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("已观察到云端版本：${observedHead?.let { "…${it.takeLast(6)}" } ?: "尚无云端版本"}。确认时会再次校验；若已变化则保留冲突，不覆盖当前云端版本。")
                    Text(SyncMessages.localReplaceCloudConfirmBody(
                        target = "MH3G / Android Nemessix",
                        serverEndpoint = serverEndpoint,
                        sessionActive = sessionActive,
                    ))
                }
            },
            confirmButton = {
                TextButton(onClick = onConfirm) {
                    Text("确认本地替换云端")
                }
            },
            dismissButton = {
                TextButton(onClick = onDismiss) {
                    Text("暂不处理")
                }
            },
        )
    }

    private fun formatTime(epochMillis: Long): String {
        if (epochMillis <= 0L) return "暂无"
        return SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.CHINA).format(Date(epochMillis))
    }

    private fun launchNemessixOrExplain(): String {
        val packageName = SyncScheduler.NEMESSIX_PACKAGE
        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
            ?: return SyncMessages.launchNemessixMissing(packageName)
        startActivity(launchIntent)
        return SyncMessages.launchNemessixStarted()
    }
}
