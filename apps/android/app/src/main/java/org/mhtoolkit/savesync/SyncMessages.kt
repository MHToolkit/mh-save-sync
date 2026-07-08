package org.mhtoolkit.savesync

object SyncMessages {
    private fun legacyTerm(vararg codePoints: Int): String =
        codePoints.map { it.toChar() }.joinToString(separator = "")

    private val legacyInternalTerms = listOf(
        "CAS",
        "HEAD",
        "SAF",
        "staging",
        "manifest/hash",
        "hash",
        "parent",
        "DAG",
        "fast-forward",
        "conflict",
        "device",
        "dirty",
        "watcher",
        "FileObserver",
        "FSEvents",
        "atomic replace",
        "unknown",
        "fail closed",
        legacyTerm(0x9501, 0x5b9a),
        legacyTerm(0x6807, 0x8bb0, 0x4f1a, 0x8bdd),
        legacyTerm(0x540c, 0x6b65, 0x4f1a, 0x8bdd),
    )

    fun serverLabel(endpoint: String?): String =
        endpoint.orEmpty().trim().trimEnd('/').ifBlank { "未配置服务器" }

    fun sanitizeLegacyUserCopy(value: String?, fallback: String): String {
        val text = value.orEmpty()
        return if (text.isBlank() || legacyInternalTerms.any { term -> term in text }) {
            fallback
        } else {
            text
        }
    }

    fun syncRoute(target: String, endpoint: String?): String =
        "同步路线：$target → 本机安全缓存 → ${serverLabel(endpoint)}。服务器只接收端到端加密快照；原始存档仍留在模拟器原目录。"

    fun cloudActionNeedsServer(): String =
        "云端同步未开始：还没有填写服务器地址。Mac 和 Android 必须填写同一个服务器地址后，上传、下载、恢复才会执行；当前没有同步到任何服务器。"

    fun launchNemessixStarted(): String =
        "已完成启动前检查并尝试打开 Nemessix。若云端较新或存在冲突，请先回到本工具处理；云端不可用时可继续使用本地存档，退出后再补传。"

    fun launchNemessixMissing(packageName: String): String =
        "没有找到 Nemessix App（包名 $packageName）。请手动打开 Nemessix；启动前检查结果仍有效。"

    fun launchPausedForCloudUnavailable(): String =
        "云端不可用或未配置，已暂停自动打开 Nemessix。你可以点「继续使用本地并打开 Nemessix」继续使用本地存档；本地队列会在云端恢复后再补传。"

    fun reconcileSummary(
        reason: String,
        target: String,
        endpoint: String?,
        sessionActive: Boolean = false,
    ): String {
        val server = serverLabel(endpoint)
        return when (reason) {
            "manual-upload" -> "已处理同步到服务器：$target 会先等待存档稳定、复制到本机安全缓存并校验，再上传到 $server。同步方向是本地存档 → 本机安全缓存 → 服务器。"
            "session-exit" -> "已处理退出后对账：Nemessix 停止后才允许恢复；若有稳定本地快照，会加密排队上传。"
            "user-use-local" -> localReplaceCloudProcessed(target, server, sessionActive)
            "download-cache-only" -> "已处理只下载：云端版本只进入本机安全缓存，不会覆盖正在运行的 Nemessix 存档目录。"
            "restore-cloud-head" -> "已处理恢复云端版本：目标=$target，服务器=$server。恢复只会在确认 Nemessix 已停止后执行，且会先备份当前本地存档，再从本机安全缓存恢复到原目录。"
            "restore-blocked-running" -> "已拒绝恢复：Nemessix 仍在运行，没有覆盖本地存档。请先退出游戏/模拟器，再执行云端覆盖本地。"
            "periodic" -> "已执行 15 分钟级兜底对账：无变化不会全量读取；发现变化也必须等稳定快照后再上传。"
            else -> "已执行对账：原因=$reason；同步目标=$target；不会静默覆盖本地或云端。"
        }
    }


    fun queuedPhase(reason: String): String =
        when (reason) {
            "manual-upload" -> "上传排队中"
            "download-cache-only" -> "下载排队中"
            "restore-cloud-head" -> "恢复排队中"
            "user-use-local" -> "本地替换云端待处理"
            "session-exit" -> "退出后对账排队中"
            else -> "后台对账排队中"
        }

    fun queuedNextAction(reason: String, sessionActive: Boolean): String =
        when (reason) {
            "manual-upload" -> "等待存档稳定；通过校验后会加密上传到服务器。"
            "download-cache-only" -> "只下载到本机缓存；不会覆盖 Nemessix 原目录。"
            "restore-cloud-head" -> if (sessionActive) {
                "请先退出 MH3G；运行中不会覆盖本地存档。"
            } else {
                "执行前会再次确认 Nemessix 已停止，并先备份当前本地存档。"
            }
            "user-use-local" -> if (sessionActive) {
                "已记录选择；退出并通过稳定校验后才会上传本地版本。"
            } else {
                "等待稳定校验；云端旧版本会保留为历史/冲突分支。"
            }
            "session-exit" -> "正在准备退出后对账；有稳定新快照才会上传。"
            else -> "后台会按安全规则检查变化；不会静默覆盖任何一边。"
        }

    fun completedPhase(reason: String): String =
        when (reason) {
            "manual-upload" -> "上传流程已处理"
            "download-cache-only" -> "下载流程已处理"
            "restore-cloud-head" -> "恢复流程已处理"
            "restore-blocked-running" -> "恢复已拒绝"
            "user-use-local" -> "本地替换云端已处理"
            "periodic" -> "兜底对账已处理"
            "session-exit" -> "退出后对账已处理"
            else -> "后台对账已处理"
        }

    fun completedNextAction(reason: String, sessionActive: Boolean): String =
        when (reason) {
            "manual-upload" -> "如果上传失败，队列会保留；下次云端可用时继续补传。"
            "download-cache-only" -> "需要恢复时，请确认 Nemessix 已停止后再点云端覆盖本地。"
            "restore-cloud-head" -> "请启动 Nemessix 检查游戏能否读取恢复后的存档。"
            "restore-blocked-running" -> "请先退出 MH3G，再执行云端覆盖本地。"
            "user-use-local" -> if (sessionActive) {
                "继续游玩即可；退出后再上传稳定快照。"
            } else {
                "请确认另一台设备是否也有修改；有冲突时继续保留两边历史。"
            }
            "periodic" -> "无需操作；有变化才会继续排队处理。"
            "session-exit" -> "保持网络可用；后台会上传稳定快照。"
            else -> "查看最近同步说明；必要时手动重试。"
        }

    fun manualUploadQueued(target: String, serverEndpoint: String): String =
        "已排队：同步到服务器。同步方向是 $target → 本机安全缓存 → ${serverLabel(serverEndpoint)}；后台会先等待存档稳定、复制并校验，再端到端加密上传。"

    fun downloadCacheQueued(serverEndpoint: String): String =
        "已排队：只下载云端到本机缓存。同步方向是 ${serverLabel(serverEndpoint)} → 本机安全缓存；不会覆盖 Nemessix 原目录，真正恢复前会再次确认模拟器已停止并先备份本地存档。"

    fun prelaunchRemoteDecisionHint(): String =
        "发现云端版本后，请先选一个动作：只下载到本机缓存、云端覆盖本地，或继续使用本地并打开 Nemessix。"

    fun continueLocalRiskHint(): String =
        "继续使用本地表示这次先不恢复云端；如果另一台设备之后也修改，会进入冲突待处理，需要你选择保留哪一边。"

    fun continueLocalLaunchQueued(): String =
        "已选择继续使用本地存档。当前不会从云端覆盖本地，也不会把未验证中间态上传；退出 MH3G 后再对账补传。"

    fun restoreCloudHeadQueued(serverEndpoint: String): String =
        "已排队：云端覆盖本地。服务器=${serverLabel(serverEndpoint)}；执行前必须确认 Nemessix 已停止，并先备份当前本地存档。"

    fun localReplaceCloudQueued(
        target: String,
        serverEndpoint: String,
        sessionActive: Boolean,
    ): String {
        val server = serverLabel(serverEndpoint)
        return if (sessionActive) {
            "已排队：本地替换云端（退出后上传）。MH3G 正在玩，不会上传正在写入的中间态；退出并通过稳定校验后，才会把 $target 上传到 $server。云端旧版本会保留为历史/冲突分支。"
        } else {
            "已排队：本地替换云端。同步方向是 $target → 本机安全缓存 → $server；只有稳定快照通过校验后才会上传，云端旧版本会保留为历史/冲突分支。"
        }
    }

    private fun localReplaceCloudProcessed(
        target: String,
        server: String,
        sessionActive: Boolean,
    ): String =
        if (sessionActive) {
            "已记录冲突选择：本地替换云端。MH3G 仍在运行，没有上传正在写入的中间态；退出并通过稳定校验后，才会把 $target 上传到 $server。云端旧版本会保留为历史/冲突分支。"
        } else {
            "已处理冲突选择：本地替换云端。$target 的稳定快照会上传到 $server；云端旧版本保留为历史/冲突分支。"
        }

    fun restoreBlockedRunning(): String =
        "已拒绝恢复：Nemessix 仍在运行，没有覆盖本地存档。请先点击“我已退出 MH3G”或退出模拟器，再执行云端覆盖本地。"

    fun sessionStartSummary(): String =
        "我正在玩 MH3G：已开启本地存档保护。运行中只允许上传已验证稳定快照，禁止云端覆盖本地目录。"

    fun sessionExitSummary(): String =
        "我已退出 MH3G：退出后对账已排队。若本地有稳定新快照，会加密上传到服务器。"

    fun activeSessionToggleLabel(sessionActive: Boolean): String =
        if (sessionActive) {
            "我已退出 MH3G（开始对账上传）"
        } else {
            "我正在玩 MH3G（保护本地存档）"
        }

    fun activeSessionChannelName(): String =
        "Nemessix 游戏运行保护"

    fun activeSessionNotificationTitle(): String =
        "MH 云存档：游戏运行保护中"

    fun activeSessionNotificationText(): String =
        "正在玩 MH3G：禁止云端覆盖本地；退出后再对账上传稳定快照"
}
