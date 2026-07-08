# MH 云存档同步用户流（一期中文 Alpha）

- 访问日期：2026-07-07
- 范围：办公室 macOS Nemessix 与回家 Android Nemessix/Azahar/Citra-family 的云存档体验。
- 目标：用户必须清楚知道“同步到哪里、什么时候上传、什么时候下载、冲突怎么选、云端不可用能不能继续玩”。

## 核心心智

1. **Mac 和 Android 都同步到同一个服务器**：客户端填写同一个 `MH_SAVE_SYNC_SERVER_URL` / Android 服务器地址。当前隔离 Alpha 测试 API 是 `http://8.130.112.207:39082`；服务器只保存端到端加密后的 chunk、manifest 和最小图元数据。
2. **启动前先检查，不静默覆盖**：启动 MH3G 前做 pre-launch check。远端较新、冲突、云端不可用都必须在 UI 中可见。
3. **watcher 不是上传器**：FSEvents/FileObserver/SAF reconciliation 只标记 dirty；只有 save-complete、模拟器退出、定时对账或手动同步会创建稳定快照候选；候选必须经过 debounce、稳定指纹、只读 staging copy、manifest/hash 和 adapter consistency validation。
4. **运行中不恢复**：模拟器运行时禁止把远端内容覆盖到原存档目录。下载只进入 local CAS；恢复必须等模拟器停止，并先快照当前本地状态。
5. **冲突是分支，不是 latest-wins**：本地/云端两边都从同一 parent 分叉时，列出 device、时间、parent、size/hash，让用户选择“本地替换云端”“云端覆盖本地”或暂不处理；未选择前不推进 HEAD。
6. **手动同步必须可解释**：CLI/桌面/Android 同步动作都要显示 `server_url`、`sync_target`、`logical_save_id`、上传设备、云端旧 HEAD、新 HEAD、`outcome` 与 `conflict_snapshot`；恢复动作还要显示下载的 `snapshot_id`、备份位置和恢复前置条件。只显示“同步成功”不算合格。

## 办公室 Mac → 回家 Android

1. Mac 菜单栏/CLI 显示：
   - 同步到服务器：来自 `MH_SAVE_SYNC_SERVER_URL`。
   - 当前对象：`MH3G / macOS Nemessix`。
   - 存档目录提示：`~/Library/Application Support/Nemessix/.../data/00000001/`。
2. 启动 MH3G 前：
   - 云端可用且无差异：允许启动。
   - 云端较新：先下载到 local CAS；用户确认后才恢复到本地。
   - 冲突：展示 Mac 与 Android 两边版本信息。
   - 云端不可用：明确“可以继续使用本地；退出后排队补传”。
3. 退出 Nemessix 后：
   - process lifecycle / helper 触发 reconcile。
   - 稳定快照通过校验后上传到服务器。
   - 上传失败不破坏本地；队列保留。
   - 当前 CLI 证据命令：`mh-save server-upload --server-url <服务器> --root <存档目录> --secret-hex <恢复密钥hex> --device-id office-mac`；输出会写明“已上传到服务器”、云端 HEAD 和冲突分支。
4. Android 打开 App：
   - 填同一个服务器地址。
   - 授权 Android Nemessix SAF 存档目录。
   - 点击“启动前检查”。Android 会先请求服务器 `/ready`，再查询 MH3G 逻辑存档云端 HEAD；云端不可用时明确可继续本地，云端有 HEAD 时先下载到缓存；点“恢复云端到本地（需停止 Nemessix）”后，确认 Nemessix 已停止、先备份当前本地存档再恢复。运行中会 fail closed 并提示没有覆盖本地存档。

## 回家 Android → 办公室 Mac

1. Android 前台服务标记 Nemessix 活跃会话：通知显示“运行中禁止云端覆盖本地；退出后再对账上传稳定快照”。
2. 退出/手动同步后：
   - OneTimeWorkRequest 处理退出后上传。
   - PeriodicWorkRequest 做 15 分钟级兜底，默认 Wi-Fi only + battery-not-low。
3. Mac 下一次启动前检查：
   - 若 Android 快照 fast-forward：提示可恢复云端。
   - 若 Mac 本地也改过：进入 conflict branch UI。

## 冲突选择语义

| 用户动作 | 实际语义 | 安全要求 |
| --- | --- | --- |
| 云端覆盖本地 | 下载云端 snapshot 到 local CAS，然后恢复到模拟器原目录 | 模拟器停止；恢复前先备份当前本地；staging 后 atomic replace / SAF journal rollback |
| 本地替换云端 | 当前本地状态形成新的加密 snapshot 并尝试 CAS HEAD | 不删除云端旧版本；旧 HEAD 保留为历史/冲突分支 |
| 暂不处理 | 保持 conflict 状态 | 不自动推进 HEAD；不按 mtime/最新时间覆盖 |

## 云端不可用

- 用户可继续本地游玩。
- 本地 snapshot DAG、upload queue、audit 仍写入本机 SQLite WAL。
- 恢复网络后执行 missing chunk / resumable upload，再做 CAS head 判断。
- UI 必须说清“现在没有同步到服务器”，不能只显示一个失败图标。

## 当前 Alpha 边界

- Android UI 已改为中文同步工作台，第一屏「当前状态和下一步」会给出一个推荐主操作：打开 MH3G 同步、选择 Android Nemessix 存档目录、填写和 Mac 一样的服务器地址、标记「我已退出 MH3G」或执行「启动前检查」；用户不需要理解内部队列、锁、缓存或版本图术语即可知道下一步。同步工作台还能显示服务器目标、目录授权、MH3G 开关、启动前云端探测、冲突选择、游戏运行保护、恢复云端到本地（需停止 Nemessix）与后台策略；同步动作区提供明确的「本地替换云端（保留云端旧版本）」入口，运行中只记录选择并等待退出后的稳定校验，避免上传正在写入的中间态。
- Android 运行保护按钮使用玩家语言：`我正在玩 MH3G（保护本地存档）` / `我已退出 MH3G（开始对账上传）`，避免把内部 session/lock 术语暴露给用户。
- Android Alpha 允许 `http://IP:port` 自部署地址；端到端加密保护存档内容，生产入口仍应使用 TLS 反向代理后再收紧 cleartext policy。
- macOS 菜单栏 Alpha 已把同步入口前置到玩家可见动作：菜单栏标题会显示 `MH 云存档 · 设服务器/选目录/选密钥/就绪`，菜单顶部固定显示 `同步路线：MH3G / macOS Nemessix → 本机安全缓存 → 服务器地址` 和 `下一步：...`；常用动作包括 `打开同步向导（告诉我下一步）`、`设置服务器地址…`、`选择 Mac Nemessix 存档目录…`、`选择恢复密钥文件…`、`立即上传 Mac 存档到服务器`、`我已退出 MH3G：立即对账上传`、`查看云端状态`、`云端覆盖本地（先备份，需停止 Nemessix）` 和 `自动同步：退出 Nemessix 后上传`。`./scripts/build-macos-app-bundle.sh` 会把共享 Rust CLI `mh-save` 一起打进 `.app`，因此双击安装后的 `/Applications/MH Save Sync.app` 不依赖用户在终端设置 `MH_SAVE_SYNC_CLI`。自动同步只监听 Nemessix 进程从运行到退出的 session boundary，不做高频全盘轮询，也不会在运行中 live overwrite。
- CLI 已增加 `server-upload` / `server-status` / `server-restore`：用于真实 server API 的端到端上传、HEAD 查询、history/conflict 计数、云端 HEAD 下载恢复与中文结果说明；`scripts/server-sync-e2e.sh` 固化办公室/回家分叉不覆盖 HEAD、恢复云端 HEAD、运行中恢复 fail-closed 的可复现证据。
- 真实 Runtime Verified 仍以 `docs/runbooks/PHASE1_VALIDATION.md` 为准；fixture 或 UI 示例不得升级为 Runtime Verified。
