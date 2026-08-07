package org.mhtoolkit.savesync

import android.provider.Settings
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/** Organization-level semantic values for Save Sync surfaces. */
object SaveSyncDesignTokens {
    const val contractVersion = "apple-design-v1"
    val statusRailStepLabels = listOf("检查", "确认", "写入/回滚")
    val statusRailLayoutFallbacks = listOf("full-width-column", "live-region-summary")
    val screenPadding = 20.dp
    val sectionGap = 14.dp
    val contentGap = 10.dp
    const val contentMotionMillis = 240
    const val reducedMotionMillis = 0
    val statusRailPadding = 14.dp

    val success = Color(0xFF1B7F4B)
    val warning = Color(0xFF9A6700)
    val error = Color(0xFFBA1A1A)

    fun motionDurationMillis(reducedMotion: Boolean): Int =
        if (reducedMotion) reducedMotionMillis else contentMotionMillis
}

private val LightSaveSyncColors = lightColorScheme(
    primary = Color(0xFF0061A4),
    onPrimary = Color.White,
    secondary = Color(0xFF4F616E),
    tertiary = Color(0xFF625A7D),
    error = SaveSyncDesignTokens.error,
)

private val DarkSaveSyncColors = darkColorScheme(
    primary = Color(0xFF9DCAFF),
    secondary = Color(0xFFB7C9D6),
    tertiary = Color(0xFFD0C4F4),
    error = Color(0xFFFFB4AB),
)

@Composable
fun SaveSyncTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) DarkSaveSyncColors else LightSaveSyncColors,
        content = content,
    )
}

@Composable
fun rememberSaveSyncMotionDurationMillis(): Int {
    val context = LocalContext.current
    val reducedMotion = remember(context) {
        runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            ) == 0f
        }.getOrDefault(false)
    }
    return SaveSyncDesignTokens.motionDurationMillis(reducedMotion)
}
