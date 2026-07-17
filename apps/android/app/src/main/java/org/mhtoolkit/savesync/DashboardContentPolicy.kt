package org.mhtoolkit.savesync

enum class RestoreFailureKind {
    EMULATOR_RUNNING,
    NEMESSIX_AUTH_OR_VERSION,
    NEMESSIX_UNAVAILABLE,
    OTHER,
}

data class RestoreFailureGuidance(
    val reason: String,
    val summary: String,
    val phase: String,
    val action: String,
    val error: String,
)

/** Keeps the first screen focused on user tasks; explanations live in Help. */
object DashboardContentPolicy {
    val primarySections = listOf("存档状态", "快速同步", "启动游戏", "最近记录")
    const val uploadLabel = "上传手机存档"
    const val restoreLabel = "恢复云端存档"
    const val launchLabel = "检查并打开 Nemessix"
    const val settingsLabel = "设置"
    const val helpLabel = "使用帮助"
    val helpTopics = listOf("第一次怎么用", "冲突怎么处理", "同步安全吗")

    fun launchStatus(reason: String): String = when (reason) {
        "prelaunch-checking" -> "正在检查云端…"
        "prelaunch-synced" -> "手机与云端一致，可以启动"
        "prelaunch-remote-advanced" -> "云端有新进度，建议先恢复"
        "prelaunch-local-changed" -> "手机有新进度，建议先上传"
        "prelaunch-diverged" -> "手机与云端都有新进度，请选择方向"
        "prelaunch-unknown" -> "首次检查，请选择使用手机或云端版本"
        "prelaunch-local-unavailable" -> "无法读取手机存档，请检查目录授权"
        "prelaunch-emulator-running" -> "Nemessix 正在运行，不执行恢复"
        "prelaunch-cloud-unavailable" -> "云端暂不可用，可继续使用本地"
        "prelaunch-no-server" -> "未设置服务器，可继续使用本地"
        "prelaunch-key-required" -> "未导入恢复密钥，可继续使用本地"
        "prelaunch-no-remote-head" -> "云端暂无存档，可以启动"
        "prelaunch-up-to-date" -> "云端已检查，可以启动"
        else -> "启动前会先检查云端"
    }

    fun restoreFailureKind(error: Throwable): RestoreFailureKind {
        val codes = generateSequence(error as Throwable?) { it.cause }.mapNotNull { it.message }.toSet()
        return when {
            "nemessix_quiescence_emulator_running" in codes -> RestoreFailureKind.EMULATOR_RUNNING
            codes.any { it in NEMESSIX_AUTH_OR_VERSION_ERRORS } -> RestoreFailureKind.NEMESSIX_AUTH_OR_VERSION
            codes.any { it in NEMESSIX_UNAVAILABLE_ERRORS } -> RestoreFailureKind.NEMESSIX_UNAVAILABLE
            else -> RestoreFailureKind.OTHER
        }
    }

    fun restoreFailureGuidance(error: Throwable): RestoreFailureGuidance =
        when (restoreFailureKind(error)) {
            RestoreFailureKind.EMULATOR_RUNNING -> RestoreFailureGuidance(
                "restore-blocked-running", "Nemessix 仍在后台运行，恢复已暂停，没有继续写入本地存档。",
                "等待退出游戏", "请从最近任务退出 Nemessix，再点“恢复云端存档”。",
                "Nemessix 尚未完全退出",
            )
            RestoreFailureKind.NEMESSIX_AUTH_OR_VERSION -> RestoreFailureGuidance(
                "restore-nemessix-incompatible", "Nemessix 未授权本次恢复，或双方版本不兼容；未覆盖本地存档。",
                "需要更新应用", "请更新 Nemessix 和 MH Save Sync 后重试。",
                "Nemessix 授权或版本不兼容",
            )
            RestoreFailureKind.NEMESSIX_UNAVAILABLE -> RestoreFailureGuidance(
                "restore-nemessix-unavailable", "当前 Nemessix 未提供安全恢复接口；未覆盖本地存档。",
                "需要更新 Nemessix", "请安装支持云存档恢复的新版 Nemessix 后重试。",
                "未找到 Nemessix 安全恢复接口",
            )
            RestoreFailureKind.OTHER -> RestoreFailureGuidance(
                "restore-cloud-head-failed", "云端恢复未完成；没有静默覆盖本地存档。",
                "恢复失败", "保持 Nemessix 关闭并重试；未完成的安全恢复会在下次继续处理。",
                "恢复失败，请重试",
            )
        }

    fun status(authorized: Boolean, gameEnabled: Boolean, serverConfigured: Boolean, sessionActive: Boolean): String =
        when {
            !gameEnabled -> "MH3G 同步已暂停"
            !authorized -> "需要设置存档目录"
            !serverConfigured -> "需要设置服务器"
            sessionActive -> "游玩中 · 本地已保护"
            else -> "可以同步"
        }

    private val NEMESSIX_AUTH_OR_VERSION_ERRORS = setOf(
        "nemessix_quiescence_unauthorized",
        "nemessix_quiescence_protocol_mismatch",
        "nemessix_quiescence_untrusted_emulator",
    )
    private val NEMESSIX_UNAVAILABLE_ERRORS = setOf(
        "nemessix_quiescence_unavailable",
        "nemessix_quiescence_untrusted_provider",
    )
}
