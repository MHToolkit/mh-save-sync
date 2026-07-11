package org.mhtoolkit.savesync

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DashboardContentPolicyTest {
    @Test
    fun `default dashboard keeps only task-oriented sections visible`() {
        assertEquals(
            listOf("存档状态", "快速同步", "启动游戏", "最近记录"),
            DashboardContentPolicy.primarySections,
        )
        assertFalse(DashboardContentPolicy.primarySections.contains("端到端加密"))
        assertFalse(DashboardContentPolicy.primarySections.contains("Android Nemessix 存档目录"))
    }

    @Test
    fun `primary actions use short outcome-oriented Chinese labels`() {
        assertEquals("上传手机存档", DashboardContentPolicy.uploadLabel)
        assertEquals("恢复云端存档", DashboardContentPolicy.restoreLabel)
        assertEquals("检查并打开 Nemessix", DashboardContentPolicy.launchLabel)
        assertTrue(DashboardContentPolicy.uploadLabel.length <= 8)
        assertTrue(DashboardContentPolicy.restoreLabel.length <= 8)
    }

    @Test
    fun `long explanations belong to help instead of dashboard`() {
        assertEquals("设置", DashboardContentPolicy.settingsLabel)
        assertEquals("使用帮助", DashboardContentPolicy.helpLabel)
        assertTrue(DashboardContentPolicy.helpTopics.contains("冲突怎么处理"))
        assertTrue(DashboardContentPolicy.helpTopics.contains("同步安全吗"))
    }

    @Test
    fun `status card uses compact labels instead of paragraphs`() {
        assertEquals("需要设置存档目录", DashboardContentPolicy.status(false, true, true, false))
        assertEquals("需要设置服务器", DashboardContentPolicy.status(true, true, false, false))
        assertEquals("游玩中 · 本地已保护", DashboardContentPolicy.status(true, true, true, true))
        assertEquals("可以同步", DashboardContentPolicy.status(true, true, true, false))
        assertEquals("MH3G 同步已暂停", DashboardContentPolicy.status(true, false, true, false))
    }
}
