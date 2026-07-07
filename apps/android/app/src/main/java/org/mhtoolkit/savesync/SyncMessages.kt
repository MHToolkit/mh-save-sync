package org.mhtoolkit.savesync

object SyncMessages {
    fun serverLabel(endpoint: String?): String =
        endpoint.orEmpty().trim().ifBlank { "未配置服务器" }

    fun syncRoute(target: String, endpoint: String?): String =
        "同步路线：$target → 本机 staging/CAS 缓存 → ${serverLabel(endpoint)}。服务器只接收端到端加密快照；原始存档仍留在模拟器原目录。"

    fun cloudActionNeedsServer(): String =
        "云端同步未开始：还没有填写服务器地址。Mac 和 Android 必须填写同一个服务器地址后，上传/下载/恢复才会执行。"

    fun launchNemessixStarted(): String =
        "已完成启动前检查并尝试打开 Nemessix。若云端较新或存在冲突，请先回到本工具处理；云端不可用时可继续使用本地存档，退出后再补传。"

    fun launchNemessixMissing(packageName: String): String =
        "没有找到 Nemessix App（包名 $packageName）。请手动打开 Nemessix；启动前检查结果仍有效。"

    fun launchPausedForCloudUnavailable(): String =
        "云端不可用或未配置，已暂停自动打开 Nemessix。你可以手动打开 Nemessix 继续使用本地存档；本地队列会在云端恢复后再补传。"

    fun reconcileSummary(reason: String, target: String, endpoint: String?): String {
        val server = serverLabel(endpoint)
        return when (reason) {
            "manual-upload" -> "已处理手动上传：$target 会经过稳定窗口、staging 复制、manifest/hash 校验后，上传到 $server。"
            "session-exit" -> "已处理退出后对账：Nemessix 停止后才允许恢复；若有稳定本地快照，会加密排队上传。"
            "user-use-local" -> "已处理冲突选择：本地版本作为新的当前快照上传；云端旧版本保留为历史/冲突分支。"
            "download-cache-only" -> "已处理只下载：云端快照只进入本地缓存，不会覆盖正在运行的 Nemessix 存档目录。"
            "restore-cloud-head" -> "已处理恢复云端 HEAD：目标=$target，服务器=$server。恢复只会在确认 Nemessix 已停止后执行，且会先备份当前本地存档，再从本地缓存/staging 提交到原目录。"
            "restore-blocked-running" -> "已拒绝恢复：Nemessix 仍在运行，没有覆盖本地存档。请先退出游戏/模拟器，再执行云端覆盖本地。"
            "periodic" -> "已执行 15 分钟级兜底对账：无变化不会全量读取；发现 dirty 也必须等稳定快照后再上传。"
            else -> "已执行对账：原因=$reason；同步目标=$target；不会静默覆盖本地或云端。"
        }
    }

    fun restoreCloudHeadQueued(serverEndpoint: String): String =
        "已排队：恢复云端 HEAD 到本地。服务器=${serverLabel(serverEndpoint)}；执行前必须确认 Nemessix 已停止，并先备份当前本地存档。"

    fun restoreBlockedRunning(): String =
        "已拒绝恢复：Nemessix 仍在运行，没有覆盖本地存档。请先标记会话结束或退出模拟器。"
}
