package org.mhtoolkit.savesync

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
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
        SyncScheduler.ensurePeriodic(this)
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
        var sessionActive by remember { mutableStateOf(false) }
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
        var launchGate by remember {
            mutableStateOf(
                preferences.getString(
                    SyncScheduler.LAUNCH_GATE_SUMMARY,
                    "未检查。启动 MH3G 前点「启动前检查」。",
                ).orEmpty()
            )
        }
        var conflictVisible by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
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

        if (conflictVisible) {
            ConflictDialog(
                onDismiss = { conflictVisible = false },
                onUseRemote = {
                    conflictVisible = false
                    launchGate = "已选择：云端覆盖本地。实际恢复会先下载到本地缓存，确认 Nemessix 已停止，再备份当前本地存档，然后替换。"
                    preferences.edit()
                        .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                        .putString(SyncScheduler.LAST_SYNC_SUMMARY, launchGate)
                        .putString(SyncScheduler.LAST_SYNC_REASON, "user-use-remote")
                        .apply()
                    lastSummary = launchGate
                },
                onUseLocal = {
                    conflictVisible = false
                    SyncScheduler.enqueueImmediate(this@MainActivity, "user-use-local")
                    launchGate = "已选择：本地替换云端。当前本地存档会形成新的加密快照；云端旧版本保留在历史/冲突分支中。"
                    preferences.edit()
                        .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                        .putString(SyncScheduler.LAST_SYNC_SUMMARY, launchGate)
                        .putString(SyncScheduler.LAST_SYNC_REASON, "user-use-local")
                        .apply()
                    lastSummary = launchGate
                },
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
                Text("MH 云存档同步", style = MaterialTheme.typography.headlineMedium)
                Text(
                    "一期中文 Alpha：办公室 Mac 和回家 Android 都把 MH3G 存档同步到同一个服务器；不做静默覆盖。",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    SyncMessages.syncRoute(
                        preferences.getString(
                            SyncScheduler.LAST_SYNC_TARGET,
                            "MH3G / Android Nemessix",
                        ).orEmpty(),
                        serverEndpoint,
                    ),
                    style = MaterialTheme.typography.bodySmall,
                )

                CardSection("同步到哪里") {
                    OutlinedTextField(
                        value = serverEndpoint,
                        onValueChange = {
                            serverEndpoint = it
                            preferences.edit()
                                .putString(SyncScheduler.SERVER_ENDPOINT, it.trim())
                                .apply()
                        },
                        label = { Text("服务器地址") },
                        placeholder = { Text("例如 http://192.168.1.10:18080") },
                        supportingText = {
                            Text("Mac、Android 都填同一个地址；服务器只保存端到端加密后的快照。")
                        },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    StatusLine(
                        "当前目标",
                        preferences.getString(
                            SyncScheduler.LAST_SYNC_TARGET,
                            "MH3G / Android Nemessix",
                        ).orEmpty(),
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text("MH3G 同步开关", fontWeight = FontWeight.Medium)
                            Text("关闭后不会自动上传/恢复该游戏，历史版本仍保留。")
                        }
                        Switch(
                            checked = gameEnabled,
                            onCheckedChange = {
                                gameEnabled = it
                                preferences.edit()
                                    .putBoolean(SyncScheduler.GAME_MH3G_ENABLED, it)
                                    .apply()
                            },
                        )
                    }
                }

                CardSection("Android Nemessix 存档目录") {
                    StatusLine("目录权限", if (authorized) "已授权 SAF 目录" else "未授权，无法读取/恢复存档")
                    Button(
                        onClick = { folderPicker.launch(null) },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (authorized) "更换 Nemessix 存档目录" else "选择 Android Nemessix 存档目录")
                    }
                    Text(
                        "请选择 Nemessix 的共享存档根目录，例如 Games/Nemessix。工具不要求 root，不读取其他 App 沙盒。",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }

                CardSection("启动 MH3G 前") {
                    StatusLine("检查结果", launchGate)
                    Button(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            launchGate = "正在检查 ${SyncMessages.serverLabel(serverEndpoint)} 的 /ready 与 MH3G 云端 HEAD；不会修改本地存档。"
                            preferences.edit()
                                .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                                .putString(SyncScheduler.LAST_SYNC_REASON, "prelaunch-checking")
                                .apply()
                            scope.launch {
                                val result = SyncServerProbe.checkPrelaunch(
                                    serverEndpoint = serverEndpoint,
                                    emulatorRunning = sessionActive,
                                )
                                launchGate = result.summary
                                lastSummary = result.summary
                                preferences.edit()
                                    .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .putString(SyncScheduler.LAST_SYNC_REASON, result.reason)
                                    .apply()
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("启动前检查")
                    }
                    OutlinedButton(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            launchGate = "正在启动前检查；检查完成前不会打开 Nemessix，也不会修改本地存档。"
                            preferences.edit()
                                .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                                .putString(SyncScheduler.LAST_SYNC_REASON, "launch-precheck")
                                .apply()
                            scope.launch {
                                val result = SyncServerProbe.checkPrelaunch(
                                    serverEndpoint = serverEndpoint,
                                    emulatorRunning = sessionActive,
                                )
                                launchGate = result.summary
                                lastSummary = if (result.remoteHead != null) {
                                    "已发现云端 HEAD=${result.remoteHead}。为避免覆盖风险，暂不自动打开 Nemessix；请先选择下载/恢复或继续本地。"
                                } else if (!result.cloudReachable) {
                                    SyncMessages.launchPausedForCloudUnavailable()
                                } else {
                                    launchNemessixOrExplain()
                                }
                                preferences.edit()
                                    .putString(SyncScheduler.LAUNCH_GATE_SUMMARY, launchGate)
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .putString(SyncScheduler.LAST_SYNC_REASON, "launch-nemessix")
                                    .apply()
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("检查后打开 Nemessix")
                    }
                    OutlinedButton(
                        enabled = authorized && gameEnabled,
                        onClick = { conflictVisible = true },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("查看冲突处理示例")
                    }
                    Text(
                        "云端较新时会提示先下载/恢复；云端不可用时会明确提示是否继续使用本地，之后再补传。",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }

                CardSection("同步动作") {
                    Button(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            if (serverEndpoint.isBlank()) {
                                lastSummary = SyncMessages.cloudActionNeedsServer()
                            } else {
                                SyncScheduler.enqueueImmediate(this@MainActivity, "manual-upload")
                                lastSummary = "已排队：立即上传本地存档。后台会先等待稳定窗口、只读复制到 staging、校验 manifest/hash，再加密上传到 ${SyncMessages.serverLabel(serverEndpoint)}。"
                            }
                            preferences.edit()
                                .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                .putString(SyncScheduler.LAST_SYNC_REASON, "manual-upload")
                                .apply()
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("立即上传本地存档")
                    }
                    OutlinedButton(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            if (serverEndpoint.isBlank()) {
                                lastSummary = SyncMessages.cloudActionNeedsServer()
                            } else {
                                SyncScheduler.enqueueImmediate(this@MainActivity, "download-cache-only")
                                lastSummary = "已排队：只从 ${SyncMessages.serverLabel(serverEndpoint)} 下载云端到本地缓存。不会在 Nemessix 运行时覆盖原目录；恢复前会先备份当前本地存档。"
                            }
                            preferences.edit()
                                .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                .putString(SyncScheduler.LAST_SYNC_REASON, "download-cache-only")
                                .apply()
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("只下载云端到本地缓存")
                    }
                    OutlinedButton(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            if (serverEndpoint.isBlank()) {
                                lastSummary = SyncMessages.cloudActionNeedsServer()
                                preferences.edit()
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .putString(SyncScheduler.LAST_SYNC_REASON, "restore-no-server")
                                    .apply()
                            } else if (sessionActive) {
                                lastSummary = SyncMessages.restoreBlockedRunning()
                                preferences.edit()
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .putString(SyncScheduler.LAST_SYNC_REASON, "restore-blocked-running")
                                    .apply()
                            } else {
                                SyncScheduler.enqueueImmediate(this@MainActivity, "restore-cloud-head")
                                lastSummary = SyncMessages.restoreCloudHeadQueued(serverEndpoint)
                                preferences.edit()
                                    .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                    .putString(SyncScheduler.LAST_SYNC_REASON, "restore-cloud-head")
                                    .apply()
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text("恢复云端到本地（需停止 Nemessix）")
                    }
                    OutlinedButton(
                        enabled = authorized && gameEnabled,
                        onClick = {
                            if (sessionActive) {
                                stopService(Intent(this@MainActivity, ActiveSessionService::class.java))
                                lastSummary = "已标记 Nemessix 会话结束：退出后对账已排队。若本地有稳定新快照，会加密上传到服务器。"
                            } else {
                                ContextCompat.startForegroundService(
                                    this@MainActivity,
                                    Intent(this@MainActivity, ActiveSessionService::class.java),
                                )
                                lastSummary = "已标记 Nemessix 会话开始：运行中只允许上传已验证稳定快照，禁止云端覆盖本地目录。"
                            }
                            sessionActive = !sessionActive
                            preferences.edit()
                                .putString(SyncScheduler.LAST_SYNC_SUMMARY, lastSummary)
                                .putString(
                                    SyncScheduler.LAST_SYNC_REASON,
                                    if (sessionActive) "session-start" else "session-exit",
                                )
                                .apply()
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (sessionActive) "标记 Nemessix 会话结束" else "标记 Nemessix 会话开始")
                    }
                }

                CardSection("最近状态") {
                    StatusLine("最近同步", lastSummary)
                    StatusLine(
                        "最近时间",
                        formatTime(preferences.getLong(SyncScheduler.LAST_SYNC_UNIX_MS, 0L)),
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text("默认仅 Wi-Fi 上传", fontWeight = FontWeight.Medium)
                            Text("同时要求电量不是低电量；Android 周期任务最低 15 分钟级兜底。")
                        }
                        Switch(
                            checked = wifiOnly,
                            onCheckedChange = {
                                wifiOnly = it
                                preferences.edit().putBoolean(SyncScheduler.WIFI_ONLY, it).apply()
                                SyncScheduler.ensurePeriodic(this@MainActivity)
                            },
                        )
                    }
                }

                Text(
                    "底线：文件变化只标记 dirty；watcher 不直接上传。恢复只在模拟器停止后执行，且恢复前一定先快照当前状态。",
                    style = MaterialTheme.typography.bodySmall,
                )
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
    private fun ConflictDialog(
        onDismiss: () -> Unit,
        onUseRemote: () -> Unit,
        onUseLocal: () -> Unit,
    ) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text("发现本地与云端冲突") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("不会按最新时间自动覆盖。请选择保留哪一边作为当前版本，另一边保留为历史/冲突分支。")
                    Text("本地：Android Nemessix · 今天 21:18 · 47 KB · parent=mac-上一版")
                    Text("云端：Mac Nemessix · 今天 18:02 · 53 KB · parent=mac-上一版")
                    Text("二进制游戏存档不做语义合并；只能选择一方或复制为分支。")
                }
            },
            confirmButton = {
                TextButton(onClick = onUseRemote) {
                    Text("云端覆盖本地")
                }
            },
            dismissButton = {
                TextButton(onClick = onUseLocal) {
                    Text("本地替换云端")
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
