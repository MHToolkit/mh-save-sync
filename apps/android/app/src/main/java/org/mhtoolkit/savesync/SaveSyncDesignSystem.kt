package org.mhtoolkit.savesync

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** Organization-level semantic values for Save Sync surfaces. */
object SaveSyncDesignTokens {
    const val contractVersion = "apple-design-v1"
    val screenPadding = 20.dp
    val sectionGap = 14.dp
    val contentGap = 10.dp
    const val contentMotionMillis = 240

    val success = Color(0xFF1B7F4B)
    val warning = Color(0xFF9A6700)
    val error = Color(0xFFBA1A1A)
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
