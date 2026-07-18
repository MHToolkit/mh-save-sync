package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveSyncUiStateTest {
    @Test
    fun `conflict state is announced as blocking and keeps an explicit next action`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "等待确认",
            error = "",
            pendingUploads = 0,
            conflictCount = 2,
            sessionActive = false,
        )

        assertEquals(SaveSyncUiTone.Warning, presentation.tone)
        assertTrue(presentation.isBlocking)
        assertEquals("选择上传本地或恢复云端；不会自动覆盖", presentation.nextAction)
    }

    @Test
    fun `offline queue state explains that work is retained`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "上传排队中",
            error = "",
            pendingUploads = 3,
            conflictCount = 0,
            sessionActive = false,
        )

        assertEquals(SaveSyncUiTone.Neutral, presentation.tone)
        assertEquals("3 项等待网络恢复后续传", presentation.status)
        assertTrue(presentation.nextAction.contains("不会静默覆盖"))
    }

    @Test
    fun `error state is actionable without claiming data was changed`() {
        val presentation = SaveSyncUiStatePresentation.from(
            phase = "上传失败",
            error = "同步失败",
            pendingUploads = 1,
            conflictCount = 0,
            sessionActive = false,
        )

        assertEquals(SaveSyncUiTone.Error, presentation.tone)
        assertTrue(presentation.isBlocking)
        assertEquals("检查网络、密钥和目录授权后重试；本地与云端均未被静默覆盖", presentation.nextAction)
    }
}
