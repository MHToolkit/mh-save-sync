package org.mhtoolkit.savesync

enum class RestoreFailureKind {
    EMULATOR_RUNNING,
    MH_SAVE_SYNC_NOT_TRUSTED,
    NEMESSIX_PROTOCOL_MISMATCH,
    NEMESSIX_UNTRUSTED,
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
            "nemessix_quiescence_unauthorized" in codes -> RestoreFailureKind.MH_SAVE_SYNC_NOT_TRUSTED
            "nemessix_quiescence_protocol_mismatch" in codes -> RestoreFailureKind.NEMESSIX_PROTOCOL_MISMATCH
            "nemessix_quiescence_untrusted_emulator" in codes -> RestoreFailureKind.NEMESSIX_UNTRUSTED
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
            RestoreFailureKind.MH_SAVE_SYNC_NOT_TRUSTED -> RestoreFailureGuidance(
                "restore-client-not-trusted", "Nemessix 未信任当前 MH Save Sync 的应用签名；尚未下载或覆盖本地存档。",
                "需要更新 MH Save Sync", "请安装正式签名迁移版；不要卸载或清除应用数据。",
                "MH Save Sync 签名未获 Nemessix 授权",
            )
            RestoreFailureKind.NEMESSIX_PROTOCOL_MISMATCH -> RestoreFailureGuidance(
                "restore-protocol-mismatch", "双方安全恢复接口版本不兼容；尚未覆盖本地存档。",
                "安全接口不兼容", "请更新 Nemessix 和 MH Save Sync 后重试。",
                "安全恢复协议不兼容",
            )
            RestoreFailureKind.NEMESSIX_UNTRUSTED -> RestoreFailureGuidance(
                "restore-nemessix-untrusted", "当前 Nemessix 的应用签名未通过安全校验；尚未覆盖本地存档。",
                "Nemessix 身份异常", "请安装 MHToolkit 正式发布的 Nemessix 后重试。",
                "Nemessix 签名未通过校验",
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

    private val NEMESSIX_UNAVAILABLE_ERRORS = setOf(
        "nemessix_quiescence_unavailable",
        "nemessix_quiescence_untrusted_provider",
    )
}
