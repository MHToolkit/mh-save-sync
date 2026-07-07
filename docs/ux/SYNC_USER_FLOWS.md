# MH 云存档同步用户流（一期中文 Alpha）

- 访问日期：2026-07-07
- 范围：办公室 macOS Nemessix 与回家 Android Nemessix/Azahar/Citra-family 的云存档体验。
- 目标：用户必须清楚知道“同步到哪里、什么时候上传、什么时候下载、冲突怎么选、云端不可用能不能继续玩”。

## 核心心智

1. **Mac 和 Android 都同步到同一个服务器**：客户端填写同一个 `MH_SAVE_SYNC_SERVER_URL` / Android 服务器地址。服务器只保存端到端加密后的 chunk、manifest 和最小图元数据。
2. **启动前先检查，不静默覆盖**：启动 MH3G 前做 pre-launch check。远端较新、冲突、云端不可用都必须在 UI 中可见。
3. **watcher 不是上传器**：FSEvents/FileObserver/SAF reconciliation 只标记 dirty；候选必须经过 debounce、稳定指纹、只读 staging copy、manifest/hash 和 adapter consistency validation。
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
   - 点击“启动前检查”。云端较新时下载到缓存；点“恢复云端到本地（需停止 Nemessix）”后，确认 Nemessix 已停止、先备份当前本地存档再恢复。运行中会 fail closed 并提示没有覆盖本地存档。

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

- Android UI 已改为中文同步工作台，能显示服务器目标、SAF 授权、MH3G 开关、启动前检查、冲突选择、会话开始/结束、恢复云端到本地（需停止 Nemessix）与后台策略。
- macOS SwiftPM 入口保留 CI CLI，同时提供 `--app` 菜单栏壳；`--server-upload` / `--server-status` / `--server-restore` 已能调用同一 Rust CLI 管线展示服务器、HEAD、冲突/恢复结果。正式签名 `.app`、LaunchAgent 和 Finder 安装包仍是后续交付项。
- CLI 已增加 `server-upload` / `server-status` / `server-restore`：用于真实 server API 的端到端上传、HEAD 查询、history/conflict 计数、云端 HEAD 下载恢复与中文结果说明；`scripts/server-sync-e2e.sh` 固化办公室/回家分叉不覆盖 HEAD、恢复云端 HEAD、运行中恢复 fail-closed 的可复现证据。
- 真实 Runtime Verified 仍以 `docs/runbooks/PHASE1_VALIDATION.md` 为准；fixture 或 UI 示例不得升级为 Runtime Verified。
