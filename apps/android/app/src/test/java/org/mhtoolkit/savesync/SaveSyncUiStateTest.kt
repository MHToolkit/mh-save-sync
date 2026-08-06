package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveSyncUiStateTest {
    @Test
    fun `conflict state is blocking and keeps an explicit next action`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "等待确认",
            error = "",
            pendingUploads = 0,
            conflictCount = 2,
            sessionActive = false,
            authorized = true,
            gameEnabled = true,
            serverConfigured = true,
        )

        assertEquals(SaveSyncUiTone.Warning, presentation.tone)
        assertTrue(presentation.isBlocking)
        assertEquals("选择上传本地或恢复云端；不会自动覆盖", presentation.nextAction)
    }

    @Test
    fun `offline queue explains retained work without claiming a write`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "上传排队中",
            error = "",
            pendingUploads = 3,
            conflictCount = 0,
            sessionActive = false,
            authorized = true,
            gameEnabled = true,
            serverConfigured = true,
        )

        assertEquals(SaveSyncUiTone.Neutral, presentation.tone)
        assertFalse(presentation.isBlocking)
        assertTrue(presentation.nextAction.contains("不会静默迁移或覆盖"))
    }

    @Test
    fun `missing directory remains blocking before server state`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "暂无后台任务",
            error = "",
            pendingUploads = 0,
            conflictCount = 0,
            sessionActive = false,
            authorized = false,
            gameEnabled = true,
            serverConfigured = false,
        )

        assertEquals("需要设置存档目录", presentation.status)
        assertTrue(presentation.isBlocking)
    }
}
