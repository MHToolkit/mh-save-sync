package org.mhtoolkit.savesync

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

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
        var status by remember {
            mutableStateOf(
                if (authorized) "Idle — waiting for save-complete or exit" else "Setup required"
            )
        }
        val folderPicker = rememberLauncherForActivityResult(
            ActivityResultContracts.OpenDocumentTree()
        ) { uri ->
            if (uri != null) {
                val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                contentResolver.takePersistableUriPermission(uri, flags)
                preferences.edit().putString(SyncScheduler.SAF_ROOT, uri.toString()).apply()
                authorized = true
                status = "Folder authorized — no upload occurs until a stable snapshot exists"
            }
        }

        Scaffold { padding ->
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Text("MH Save Sync", style = MaterialTheme.typography.headlineMedium)
                Text(
                    "Phase 1 alpha · encrypted snapshots · conflict branches · no live overwrite",
                    style = MaterialTheme.typography.bodyMedium,
                )
                StatusCard("Service", "Self-hosted endpoint configured by managed settings")
                StatusCard(
                    "Save access",
                    if (authorized) "SAF permission persisted" else "No folder permission",
                )
                StatusCard("Sync status", status)
                Button(
                    onClick = { folderPicker.launch(null) },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(if (authorized) "Change authorized folder" else "Authorize save folder")
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text("Wi-Fi only")
                        Text("Battery-not-low is always enforced")
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
                Button(
                    enabled = authorized,
                    onClick = {
                        SyncScheduler.enqueueImmediate(this@MainActivity, "manual")
                        status = "Reconcile queued — watcher never uploads directly"
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Sync now")
                }
                OutlinedButton(
                    enabled = authorized,
                    onClick = {
                        if (sessionActive) {
                            stopService(Intent(this@MainActivity, ActiveSessionService::class.java))
                            status = "Session ended — exit reconcile queued"
                        } else {
                            ContextCompat.startForegroundService(
                                this@MainActivity,
                                Intent(this@MainActivity, ActiveSessionService::class.java),
                            )
                            status = "Session active — remote restore is locked"
                        }
                        sessionActive = !sessionActive
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(if (sessionActive) "End emulator session" else "Start emulator session")
                }
                Spacer(Modifier.height(4.dp))
                Text(
                    "Restore is allowed only after the emulator stops and after the current " +
                        "directory is snapshotted. Permission loss fails closed.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }

    @Composable
    private fun StatusCard(title: String, value: String) {
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Text(title, style = MaterialTheme.typography.labelLarge)
                Text(value, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}
