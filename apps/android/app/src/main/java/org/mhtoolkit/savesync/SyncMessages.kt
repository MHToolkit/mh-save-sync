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

    fun officeHomeFlowSteps(endpoint: String?): List<String> {
        val server = serverLabel(endpoint)
        return listOf(
            "办公室 Mac：退出 MH3G 后生成稳定快照并上传到 $server；上传失败也不会破坏 Mac 本地存档。",
            "回家 Android：填写同一个服务器地址，点启动前检查；云端可用时先下载到本机缓存，恢复前会先备份本地。",
            "两边都改过时：列为冲突分支，由你选择本地替换云端、云端覆盖本地或暂不处理，不会静默覆盖。",
        )
    }

    fun manualActionsIntro(target: String, endpoint: String?): String {
        val server = serverLabel(endpoint)
        return if (endpoint.orEmpty().trim().isBlank()) {
            "当前同步目的地：$target → 本机安全缓存 → $server。未配置服务器，点同步也不会离开这台手机；请先填写办公室 Mac 和回家 Android 共用的服务器地址。"
        } else {
            "当前同步目的地：$target → 本机安全缓存 → $server。同步到服务器会上传稳定快照；只下载云端到本机缓存不会覆盖；云端覆盖本地前会二次确认并先备份；本地替换云端会保留云端旧版本。"
        }
    }

    fun dashboardStateSummary(
        authorized: Boolean,
        gameEnabled: Boolean,
        endpoint: String?,
        sessionActive: Boolean,
    ): String {
        val server = serverLabel(endpoint)
        return when {
            !gameEnabled -> "MH3G 同步已关闭：不会自动上传、下载或恢复；历史版本仍保留。"
            !authorized -> "还不能同步：尚未授权 Android Nemessix 存档目录，工具不会读取或覆盖本地存档。"
            endpoint.orEmpty().trim().isBlank() -> "还没有同步到服务器：当前只显示本地状态。Mac 和 Android 必须填写同一个服务器地址。"
            sessionActive -> "本地存档保护中：你正在玩 MH3G 时不会从云端覆盖本地；退出后才对账上传到 $server。"
            else -> "已准备好：MH3G / Android Nemessix 会同步到 $server；先做启动前检查，再决定上传、下载或恢复。"
        }
    }

    fun dashboardNextAction(
        authorized: Boolean,
        gameEnabled: Boolean,
        endpoint: String?,
        sessionActive: Boolean,
    ): String = when {
        !gameEnabled -> "打开「MH3G 同步开关」后，再授权目录并做启动前检查。"
        !authorized -> "先点「选择 Android Nemessix 存档目录」，授权后再同步。"
        endpoint.orEmpty().trim().isBlank() -> "填写和 Mac 一样的服务器地址；未填写前不会上传到任何地方。"
        sessionActive -> "如果正在玩就继续；退出后点「我已退出 MH3G」开始对账上传。"
        else -> "点「启动前检查」查看云端版本；需要替换前会先让你确认。"
    }

    fun dashboardPrimaryActionLabel(
        authorized: Boolean,
        gameEnabled: Boolean,
        endpoint: String?,
        sessionActive: Boolean,
    ): String = when {
        !gameEnabled -> "打开 MH3G 同步"
        !authorized -> "选择 Android Nemessix 存档目录"
        endpoint.orEmpty().trim().isBlank() -> "到下方填写服务器地址"
        sessionActive -> activeSessionToggleLabel(sessionActive)
        else -> "启动前检查"
    }

    fun dashboardPrimaryActionHint(
        authorized: Boolean,
        gameEnabled: Boolean,
        endpoint: String?,
        sessionActive: Boolean,
    ): String = when {
        !gameEnabled -> "打开后仍需授权目录和服务器地址；未完成前不会同步。"
        !authorized -> "只选择 Nemessix 存档根目录；不会立刻上传或覆盖。"
        endpoint.orEmpty().trim().isBlank() -> "Mac 和 Android 填同一个服务器地址，例如自部署 API 地址；未填写前不会上传。"
        sessionActive -> "结束游玩后点这里，对账上传稳定快照；运行中不从云端覆盖本地。"
        else -> "先确认云端是否有新版本或冲突；检查不会修改本地存档。"
    }

    fun noServerPhase(): String =
        "需要服务器地址"

    fun noServerNextAction(actionLabel: String): String =
        "请先填写 Mac 和 Android 共用的服务器地址，再执行$actionLabel。"

    fun noServerError(): String =
        "未配置服务器"

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
        "发现云端版本后，请先选一个动作：只下载到本机缓存、云端覆盖本地，或继续使用本地并打开 Nemessix。若有冲突，工具会先展示文件/字节级差异；能否解析猎人名、装备、道具取决于具体游戏解析器。"

    fun continueLocalRiskHint(): String =
        "继续使用本地表示这次先不恢复云端；如果另一台设备之后也修改，会进入冲突待处理，需要你选择保留哪一边。"

    fun conflictDiffBoundary(): String =
        "当前 Alpha 已有 MH3G/3U 3DS 专用差异解析入口：先列出两边不同的文件、大小、校验摘要和变更字节段；暂不声称能语义解析猎人名、装备、道具或任务进度。后续每个游戏会独立增加解析器，不能通用猜。"

    fun continueLocalLaunchQueued(): String =
        "已选择继续使用本地存档。当前不会从云端覆盖本地，也不会把未验证中间态上传；退出 MH3G 后再对账补传。"

    fun continueLocalPhase(): String =
        "继续使用本地存档"

    fun continueLocalNextAction(): String =
        "可以先玩；退出 MH3G 后再做对账补传，云端旧版本不会被静默覆盖。"

    fun restoreCloudConfirmTitle(): String =
        "确认用云端版本恢复本地？"

    fun restoreCloudConfirmBody(serverEndpoint: String): String =
        "这会从 ${serverLabel(serverEndpoint)} 取回云端版本，并在确认 Nemessix 已停止后恢复到 Android Nemessix 存档目录。执行前会先备份当前本地存档；如果你不确定，请选择继续使用本地。"

    fun localReplaceCloudConfirmTitle(): String =
        "确认用本地版本替换云端？"

    fun localReplaceCloudConfirmBody(
        target: String,
        serverEndpoint: String,
        sessionActive: Boolean,
    ): String {
        val timing = if (sessionActive) {
            "当前正在玩 MH3G，不会立刻上传；退出并通过稳定校验后才会上传。"
        } else {
            "会先等待稳定校验，通过后才会上传。"
        }
        return "这会把 $target 作为新的云端版本上传到 ${serverLabel(serverEndpoint)}。$timing 云端旧版本会保留为历史/冲突分支，不会按时间静默覆盖。"
    }

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
