package org.mhtoolkit.savesync

/** Keeps the first screen focused on user tasks; explanations live in Help. */
object DashboardContentPolicy {
    val primarySections = listOf("存档状态", "快速同步", "启动游戏", "最近记录")
    const val uploadLabel = "上传手机存档"
    const val restoreLabel = "恢复云端存档"
    const val launchLabel = "检查并打开 Nemessix"
    const val settingsLabel = "设置"
    const val helpLabel = "使用帮助"
    val helpTopics = listOf("第一次怎么用", "冲突怎么处理", "同步安全吗")

    fun status(authorized: Boolean, gameEnabled: Boolean, serverConfigured: Boolean, sessionActive: Boolean): String =
        when {
            !gameEnabled -> "MH3G 同步已暂停"
            !authorized -> "需要设置存档目录"
            !serverConfigured -> "需要设置服务器"
            sessionActive -> "游玩中 · 本地已保护"
            else -> "可以同步"
        }
}
