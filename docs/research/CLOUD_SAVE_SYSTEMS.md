# 成熟云存档系统研究与取舍

- 状态：Phase 1 设计输入
- 研究日期 / 资料访问日期：2026-07-04
- 关键结论来源范围：平台官方文档、项目官方文档/源码、标准数据库官方文档
- 适用项目：`MHToolkit/mh-save-sync`

## 1. 研究问题与判定标准

本研究回答的不是“怎样把目录同步得最快”，而是“怎样在模拟器仍拥有原始
存档控制权、设备可能离线、写入可能非原子、远端可能故障的条件下，保证任何
自动化都不会静默丢档”。

采用机制必须同时满足：

1. **保存边界明确**：watcher 只能标记 dirty，不能直接触发上传；优先使用
   save-complete、launch、exit 等 session boundary。
2. **历史不可丢**：每次稳定状态形成不可变快照；分叉保留为 conflict branch，
   不以时间戳自动覆盖。
3. **提交可证明**：HEAD 只允许指向所有依赖对象均已持久化的快照。
4. **恢复可回滚**：恢复先备份当前状态，再 staging、验证、原子替换；模拟器
   运行时禁止恢复。
5. **状态可见、动作可控**：按游戏启停、同步状态、错误、手动上传/下载/恢复
   必须显式呈现。
6. **本地优先**：服务端故障不得影响本地游玩或原始存档；上传队列可以稍后续传。

本文中的“采用”只表示采用该系统的机制思想，不表示复用其协议、服务或
密码学实现。

证据等级说明：

- **官方资料确认**：本轮已读取相应官方页面并据此形成机制结论。
- **文档术语检查**：本轮可通过无账号的 HTTP 文本检查复核；最终自检结果见第 6 节。
- **建议复现实验**：是后续在合法账号、硬件或隔离 fixture 上的验收设计，除第 6 节
  明确记录者外，本轮未执行，不能据此把任何平台标记为 Runtime Verified。

## 2. 机制矩阵

| 系统 | 自动化边界 | 冲突/历史 | 数据落盘与提交 | 状态/手动控制 | 主要事故面 | mh-save-sync 取舍 |
|---|---|---|---|---|---|---|
| Steam Cloud / Auto-Cloud | 启动前下载、退出后上传；可在 suspend 时动态同步 | 官方页未承诺保留冲突分支 | Auto-Cloud 按配置路径同步；跨平台 Root Override | 全局/单游戏开关；按用户/游戏配额 | 把设备配置同步到其他机器；动态同步实现不完整可丢数据；会话末大文件阻塞 | **采用** session boundary、跨平台路径映射、配额意识；**不采用**裸文件镜像和隐式冲突处理 |
| Nintendo Switch Save Data Cloud | 默认自动备份；游戏运行时不备份，退出/睡眠后备份 | 单份云备份；分歧需用户选，覆盖不可恢复 | 平台托管 | 按用户/游戏 capability；逐设备下载开关；状态和手动备份/下载 | 手动上传/下载可覆盖唯一版本；另一设备未先下载会触发分歧 | **采用** per-game capability、运行期禁恢复、状态和手动动作；**拒绝**单份覆盖模型 |
| PlayStation Plus（PS5） | 关闭游戏或 Rest Mode 自动同步 | “most recent”策略，不提供通用冲突版本历史 | 平台托管 | 按游戏开关、同步状态、手动上传/下载 | 设备时钟/错误分支被判断为“最新”时存在覆盖风险 | **采用**触发边界、状态与手动动作；**拒绝** latest-wins |
| Google Play Games Saved Games | 启动/恢复时加载；离线可读写，联网后异步同步 | API 同时暴露 local/server 两版本，应用选择或合并 | Snapshot API 提交 | 应用负责冲突 UI/策略 | 应用若选“最近修改”仍会丢另一分支；异步回连产生递归冲突 | **采用**显式双版本冲突与应用决策；二进制存档只选分支/复制分支，不声称语义合并 |
| Apple iCloud Documents / `NSFileVersion` | iCloud daemon + coordinated access | current version 之外保留 unresolved conflict versions | `NSFileCoordinator` 串行协调读写/替换 | 应用发现、展示并标记冲突已解决 | 系统会选 current，但不代表用户想要；过早删除 conflict version 会丢历史 | **采用** conflict versions、协调写入、用户决议；**不采用**系统选中的 current 作为静默胜者 |
| Syncthing | watcher 延迟聚合 + 周期全扫兜底 | 冲突另存副本；可配置版本保留 | block hash 校验，先临时文件后 move-in-place | 文件夹级配置 | mtime/device-ID 决胜不适合存档；版本只覆盖远端引起的替换；大小写冲突 | **采用** dirty+reconcile、block/hash、staging/atomic replace、case collision 拒绝；**不采用**直接同步模拟器目录 |
| restic | 显式 backup 边界 | 不可变、内容寻址快照；parent 用于增量选择 | pack/blob → index → snapshot；锁区分共享/独占 | CLI 显式 backup/restore/check | 存储方可删除对象；泄露密钥需整体换库；错误 prune 可破坏历史 | **采用** immutable CAS、locks、崩溃安全顺序；一期使用 fixed chunk，不照搬 CDC |
| S3 / MinIO + PostgreSQL | 客户端分块/续传；服务端事务提交 | 对象 versioning 是额外护栏；DAG/HEAD 由 PostgreSQL 管理 | chunk → manifest → snapshot row → CAS HEAD | upload session、missing set、审计 | multipart 残片、checksum 未校验、对象存在但 DB 未提交、并发 HEAD 覆盖 | **采用** checksum、断点续传、残片回收、对象版本化、事务 CAS；对象存储不作为 HEAD 真相源 |

## 3. 平台与项目记录

### 3.1 Steam Cloud / Auto-Cloud

#### 官方资料

1. Valve, **Steam Cloud (Steamworks Documentation)**  
   URL: <https://partner.steamgames.com/doc/features/cloud?l=english>  
   访问日期：2026-07-04  
   定位：`Steam Cloud Overview`、`Notes and Best Practices`、`Initial Setup`、
   `Steam Auto-Cloud`、`Root Overrides`、`Dynamic Cloud Sync`。

#### 官方资料确认的机制

- Steam 在每次 session **之前和之后**同步：新设备在游戏启动前下载，会话内变更
  在游戏退出后上传。
- Auto-Cloud 通过 root/subdirectory/pattern/OS/recursive 声明文件集合；跨平台
  路径不同应使用一个 root 加 Root Overrides，而不是把平台目录误当成不同存档。
- 每个 cloud-enabled game 需要设置 per-user `Byte quota` 和 `Number of files`。
- 官方明确要求避免同步 video settings 等 machine-specific configuration。
- Dynamic Cloud Sync 可在 Steam Deck suspend 时上传，在另一设备启动前下载；
  唤醒原设备后会下载变化并通知应用。官方同时警告：为未正确处理回调的已发布
  build 开启此功能可能导致数据丢失。

#### 建议复现实验 / 文档定位

Steamworks 合作伙伴账号可按官方的 pre-release 流程执行：

```text
steam://open/console
testappcloudpaths <AppId>
set_spew_level 4 4
# 设备 A 启动、保存、退出；设备 B 启动前观察下载
testappcloudpaths 0
set_spew_level 0 0
```

无需账号的文档快照检查：

```bash
rtk proxy curl -fLsS 'https://partner.steamgames.com/doc/features/cloud?l=english' \
  | rtk grep -E 'before and after every session|Root Overrides|Dynamic Cloud Sync|Byte quota per user'
```

#### 优点

- session boundary 与游戏生命周期一致，避免把连续写入的中间态当成存档。
- 路径配置与跨平台 override 对多模拟器 adapter descriptor 有直接借鉴价值。
- 配额与单游戏开关使风险边界对用户可见。

#### 事故面

- Auto-Cloud 本质仍是文件复制，官方页没有可供应用保留 DAG 分叉的通用机制。
- 若错误包含图形设置、shader/cache 等设备态文件，会把一台设备配置污染到另一台。
- 会话末尾上传很多小文件或大文件会阻塞退出/重新启动。
- Dynamic Cloud Sync 若应用未处理本地文件变化通知，会在活跃/挂起状态切换时丢进度。

#### 采用 / 不采用

- **采用**：pre-launch check/pull、正常退出强制 reconcile、suspend/save-complete
  作为高置信触发、跨平台 root mapping、per-game 开关和容量配额。
- **不采用**：把 adapter 匹配到的目录直接交给通用双向文件同步；不把 mtime 或
  “云端最新”当作唯一决策依据。

### 3.2 Nintendo Switch Save Data Cloud

#### 官方资料

1. Nintendo Support, **How to Enable/Disable Automatic Save-Data Backups and Downloads**  
   URL: <https://en-americas-support.nintendo.com/app/answers/detail/a_id/41209/~/how-to-enable%2Fdisable-automatic-save-data-backups-and-downloads>  
   访问日期：2026-07-04  
   定位：默认自动备份、自动下载需逐 console 开启、并非所有软件支持。
2. Nintendo Support (Japan), **セーブデータお預かり**  
   URL: <https://support.nintendo.com/jp/nso/services/savedata-backup/index.html>  
   访问日期：2026-07-04  
   定位：`自動バックアップ`、`手動バックアップ`、`バックアップ状況の確認`。  
   注：该官方日文页明确写明正在游玩的软件不会备份，需在退出软件或主机睡眠后备份。
3. Nintendo Support, **How to Download Save Data Cloud Backups**  
   URL: <https://en-americas-support.nintendo.com/app/answers/detail/a_id/41208/~/how-to-download-save-data-cloud-backups-on-nintendo-switch>  
   访问日期：2026-07-04  
   定位：下载替换本机存档且不可恢复、按软件开关、睡眠时下载较新云版本。
4. Nintendo Support, **Which games are compatible with Save Data Cloud backup?**  
   URL: <https://www.nintendo.com/au/support/articles/which-games-are-compatible-with-save-data-cloud-backup/>  
   访问日期：2026-07-04  
   定位：eShop/软件菜单展示 capability，部分软件不支持。

#### 官方资料确认的机制

- 自动备份在订阅开始时默认开启；自动下载需要在每台 console 上分别开启。
- capability 是用户/游戏维度，不保证每个游戏支持。
- 游戏运行时不备份；退出软件或进入睡眠后才备份。手动备份同样要求先退出游戏。
- 系统展示“已备份 / 未备份 / 不支持 / 无法确认”等状态，并提供逐游戏设置和
  手动备份、手动下载。
- 官方明确：下载会替换本机存档且覆盖后不可恢复；多设备分歧时用户必须决定
  保留哪一份。

#### 建议复现实验 / 文档定位

合法拥有 Nintendo Switch Online 与支持游戏时：

1. 在主机 A 开启自动备份，游戏运行期间查看状态，确认不会完成该游戏备份。
2. 正常退出游戏或进入睡眠，确认状态转为已备份。
3. 主机 B 不开启自动下载，确认不会自动拉取；开启后在睡眠中拉取。
4. A/B 离线各自进展后联网，记录明确错误与手动选项；不要执行覆盖直到已做外部备份。

文档文本检查：

```bash
rtk proxy curl -fLsS 'https://support.nintendo.com/jp/nso/services/savedata-backup/index.html' \
  | rtk grep -E 'プレイ中|ソフト終了後|本体スリープ後|手動バックアップ'
```

#### 优点

- 自动化被限制在“软件不运行”的安全边界。
- capability、同步状态、错误状态和手动动作都可见。
- 下载开关逐设备设置，避免新设备未经用户确认就覆盖。

#### 事故面

- 通用 Save Data Cloud 以单份云备份为中心；手动上传/下载覆盖后不可恢复。
- 用户若在第二台设备未下载最新云存档就继续游戏，会产生必须人工选择的分歧。
- “自动备份默认开”若没有历史版本，损坏存档也可能很快替换唯一好版本。

#### 采用 / 不采用

- **采用**：per-game capability、逐设备自动下载设置、运行期禁 restore、明确
  pending/offline/error/conflict 状态、手动 upload/download/restore。
- **不采用**：单份云备份和不可逆覆盖；mh-save-sync 每次覆盖前必须保存当前快照，
  分歧必须生成两个 branch。

### 3.3 PlayStation Plus 云存储（PS5）

#### 官方资料

1. PlayStation Support, **PlayStation Plus cloud storage for PS5 consoles**  
   URL: <https://www.playstation.com/en-ca/support/subscriptions/ps5-ps-plus-cloud-storage/>  
   访问日期：2026-07-04  
   定位：`Automatically sync`、`Manage ... cloud storage`、
   `Check sync status of games`。

#### 官方资料确认的机制

- 关闭游戏或把 console 放入 Rest Mode 时自动同步。
- 自动同步可全局启用，并可为每个游戏单独开关。
- 用户可以查看每个游戏的同步状态，也可以显式 Upload/Download Saved Data。
- PS5 游戏的 console 与 cloud storage 同步到官方所谓的 `most recent data`；
  手动操作同样针对游戏的 most recent saved data。

#### 建议复现实验 / 文档定位

在有 PlayStation Plus 的合法测试账号上：

1. 关闭某游戏的 per-game auto-sync，保存并关闭游戏，确认云状态不变。
2. 开启后重复，确认在 close/rest-mode 边界同步。
3. 使用 `Check Sync Status of Saved Data` 记录状态，再分别测试手动 upload/download。
4. 在执行 download 前复制本机数据；官方文档没有承诺通用 conflict-version history。

无需主机的文本检查：

```bash
rtk proxy curl -fLsS 'https://www.playstation.com/en-ca/support/subscriptions/ps5-ps-plus-cloud-storage/' \
  | rtk grep -E 'close a game|rest mode|most recent|Check Sync Status|Upload/Download'
```

#### 优点

- close/rest-mode 是比原始 watcher 更可靠的稳定边界。
- per-game control、状态检查和手动动作形成完整 UX 闭环。

#### 事故面

- **推论（基于官方“most recent”措辞）**：如果一份逻辑上错误或时钟偏移的分支被
  认定为最新，自动同步可能覆盖用户真正想保留的版本。官方公开页未提供通用 DAG
  或冲突版本选择能力来消除此风险。
- 手动 Upload/Download 命名清晰，但它仍是覆盖性动作；缺少恢复前本地快照会放大误操作。

#### 采用 / 不采用

- **采用**：close/rest-mode/exit reconcile、per-game control、sync status、
  手动 upload/download。
- **不采用**：`most recent` / last-write-wins。时间只作为展示元数据，不参与
  自动选胜；HEAD 必须通过 parent CAS 形成 fast-forward，否则创建 conflict。

### 3.4 Google Play Games Saved Games

#### 官方资料

1. Android Developers, **Cloud save**  
   URL: <https://developer.android.com/games/pgs/savedgames>  
   访问日期：2026-07-04  
   定位：`Conflict resolution`、`Offline support`、Saved Games basics。
2. Android Developers, **Saved games for Android games**  
   URL: <https://developer.android.com/games/pgs/android/saved-games>  
   访问日期：2026-07-04  
   定位：`Handle saved game conflicts`、`Modify saved games`。

#### 官方资料确认的机制

- Saved Games 同时存储非结构化二进制 blob 和结构化 metadata。
- 离线时仍可本地读写；网络恢复后 Play Games Services 异步更新服务器数据。
- 多设备或离线回连会在读取时产生 conflict，API 暴露 server version 与 local
  version；应用必须选择一个版本或合并两者。
- `resolveConflict()` 后仍可能出现另一个 conflict，官方示例要求循环处理。
- 官方示例展示了按最后修改时间选择，但同时明确可向用户展示 UI；该示例不是
  对二进制游戏存档“最近必然正确”的安全保证。

#### 建议复现实验 / 文档定位

使用 Google 官方 API 测试项目和两个测试设备：

1. 两设备加载同一 snapshot 后断网，各自写入不同 marker。
2. A 先上线 commit，B 后上线并 `open()`。
3. 断言 `DataOrConflict.isConflict() == true`，保存
   `getSnapshot()` 与 `getConflictingSnapshot()` 的 metadata/bytes hash。
4. 不按 timestamp 自动 resolve；分别测试选择 A、选择 B、构造合并结果。
5. 再次 `open()`，循环直到 API 返回非 conflict。

文档文本检查：

```bash
rtk proxy curl -fLsS 'https://developer.android.com/games/pgs/android/saved-games' \
  | rtk grep -E 'Server version|Local version|must decide|resolveConflict'
```

#### 优点

- 把冲突作为协议一等结果，而不是错误或静默覆盖。
- local/server 两版本同时交给应用，支持面向领域的选择或合并。
- 离线可用、联网异步同步符合本地优先。

#### 事故面

- 应用若直接采用官方示例中的 most-recently-modified 选择，仍会删除另一个逻辑分支。
- 二进制模拟器存档通常没有可靠语义合并器；字节拼接或字段猜测会制造不可读存档。
- 异步回连期间可能连续出现冲突，单次 resolve 不是完整闭环。

#### 采用 / 不采用

- **采用**：冲突结果同时保留双方版本；允许用户选一方恢复或复制为新分支。
- **不采用**：mtime 自动选胜，也不声称能语义合并任意二进制存档。只有 adapter
  明确提供并验证了领域合并器时才允许 merge。

### 3.5 Apple iCloud Documents / `NSFileVersion`

#### 官方资料

1. Apple Developer Documentation, **NSFileVersion**  
   URL: <https://developer.apple.com/documentation/foundation/nsfileversion>  
   访问日期：2026-07-04  
   定位：`Handling Version Conflicts`、`Replacing and Deleting Versions`。
2. Apple Archive, **Resolving Document Version Conflicts**  
   URL: <https://developer.apple.com/library/archive/documentation/DataManagement/Conceptual/DocumentBasedAppPGiOS/ResolveVersionConflicts/ResolveVersionConflicts.html>  
   访问日期：2026-07-04  
   定位：`Learning About...`、`Strategies...`、`How to Tell iOS...`。
3. Apple Developer Documentation, **NSFileCoordinator**  
   URL: <https://developer.apple.com/documentation/foundation/nsfilecoordinator>  
   访问日期：2026-07-04  
   定位：Overview、Coordinating File Operations。
4. Apple Archive, **iCloud File Management**  
   URL: <https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/iCloud/iCloud.html>  
   访问日期：2026-07-04  
   定位：`File Coordination`。

#### 官方资料确认的机制

- 多个离线 writer 回连时，系统会选择一个 current file，并把其他版本标记为
  conflict；应用可用 `unresolvedConflictVersionsOfItem(at:)` 取得冲突版本。
- 应用可以 merge、选择某版本，或把版本展示给用户选择；完成后才把版本标记
  `resolved` 并清理不再需要的版本。
- `NSFileCoordinator` 在进程/对象之间协调读、写、移动、删除和替换；iCloud
  文档访问需要协调，避免应用和同步 daemon 同时写造成损坏。
- Apple 文档明确提醒：最新修改时间只有在不会丢数据时才适合用来选择；无法确定时
  应由用户选择。

#### 建议复现实验 / 源码定位

最小 macOS/iOS fixture：

1. 两设备离线编辑同一 iCloud document，写入不同固定 payload。
2. 同时联网，观察 document state conflict。
3. 枚举 `NSFileVersion.currentVersionOfItem` 和
   `unresolvedConflictVersionsOfItem`，记录版本数及内容 hash。
4. 在 `NSFileCoordinator` 的 coordinated read 内复制双方到 staging。
5. 选择一个版本替换 current 后再标记 resolved；确认未选择版本在显式清理前仍存在。

API 页面检查：

```bash
rtk proxy curl -fLsS 'https://developer.apple.com/documentation/foundation/nsfileversion' \
  | rtk grep -E 'unresolvedConflictVersions|conflict|replaceItem'
```

#### 优点

- conflict version 是可枚举、可展示、可延迟解决的一等对象。
- coordinated access 把“替换”和“普通写入”区分开，适合恢复临界区。

#### 事故面

- 系统选出的 current 只是协调结果，不代表用户意图；把 current 当成胜者仍是隐式 LWW。
- 过早设置 resolved 并删除其他版本会使错误选择不可逆。
- 单文件 document 模型不能直接解决多文件模拟器存档的一致性，仍需 snapshot manifest。

#### 采用 / 不采用

- **采用**：保留冲突版本、显式 conflict UI、staging、coordinated replace 的思想。
- **不采用**：把系统 current 或 modification date 当作自动胜者；多文件存档以完整
  snapshot 为冲突单元，不逐文件拼出混合状态。

### 3.6 Syncthing

#### 官方资料

1. Syncthing Docs, **Understanding Synchronization**  
   URL: <https://docs.syncthing.net/users/syncing.html>  
   访问日期：2026-07-04  
   定位：`Blocks`、`Scanning`、`Conflicting Changes`、
   `Case Sensitivity in File Names`、`Temporary Files`。
2. Syncthing Docs, **File Versioning**  
   URL: <https://docs.syncthing.net/users/versioning.html>  
   访问日期：2026-07-04  
   定位：versioning scope、trash/simple/staggered strategies。
3. Syncthing official source repository  
   URL: <https://github.com/syncthing/syncthing>  
   访问日期：2026-07-04  
   源码定位：官方文档 `Edit this page on GitHub` 可映射到
   `syncthing/docs`；运行实现位于官方 `syncthing/syncthing` repository。

#### 官方资料确认的机制

- 文件被切成 block，保存 offset/size/SHA-256；只获取不同 block，并在写入临时副本前
  校验 hash。
- watcher 不立即 scan，而是聚合变化；同时保留周期 full scan，因为 watcher 可能漏事件。
- 目标文件永不直接写入：先写 temporary copy，成功后 move-in-place。
- 同时修改会产生 `.sync-conflict-*` 副本并传播；大小写冲突在大小写不敏感平台上
  fail closed，而不是覆盖。
- file versioning 可按 trash/simple/staggered 保留被远端替换或删除的旧版本；
  **它不归档本机自己做出的每次修改**。

#### 建议复现实验 / 源码定位

使用两个临时目录与两个 Syncthing 测试节点：

1. 同步一个 >2 blocks 的 fixture，只修改中间 block，观察仅请求差异 block。
2. 传输中断，断言目标仍是旧完整文件，`.syncthing.*.tmp` 保留。
3. 两端离线修改同名文件，联网后断言原文件与 `.sync-conflict-*` 均存在。
4. 在大小写敏感端创建 `SAVE.bin` 与 `save.bin`，同步到大小写不敏感端，断言报告
   case conflict 且不覆盖。
5. 开启 staggered versioning，分别验证“远端替换会归档”和“本机修改不归档”。

文档文本检查：

```bash
rtk proxy curl -fLsS 'https://docs.syncthing.net/users/syncing.html' \
  | rtk grep -E 'temporary copy|sync-conflict|case conflict|full scans|SHA256'
```

#### 优点

- hash-verified block transfer、临时文件、move-in-place 是成熟的数据完整性模式。
- watcher + periodic reconciliation 说明文件事件只能是提示而非唯一真相。
- conflict copy 和 case-conflict fail-closed 明确优于静默覆盖。

#### 事故面

- Syncthing 会用 mtime/device ID 选出 global version，并把另一份重命名；它保存了
  冲突副本但仍不表达游戏存档 DAG、parent 或完整多文件事务。
- 直接同步模拟器目录会在游戏运行时改写文件，违反 mh-save-sync 恢复前提。
- versioning 默认关闭，而且只归档远端引起的替换；不能替代本地每次快照。
- 多文件存档逐文件 conflict 可能组合成模拟器从未写出过的混合状态。

#### 采用 / 不采用

- **采用**：watcher dirty + debounce + 定时对账、hash verified transfer、
  staging/atomic replace、case collision 拒绝、历史保留策略。
- **不采用**：直接双向同步原目录、mtime/device-ID 选胜、逐文件冲突。冲突单位必须是
  adapter 定义的逻辑 slot snapshot。

### 3.7 restic

#### 官方资料

1. restic Docs, **Design / Repository Format**  
   URL: <https://restic.readthedocs.io/en/v0.18.1/design.html>  
   访问日期：2026-07-04  
   定位：`Repository Format`、`Snapshots`、`Locks`、
   `Read and Write Ordering`、`Backups and Deduplication`、`Threat Model`。
2. restic Docs, **Backing up**  
   URL: <https://restic.readthedocs.io/en/v0.19.0/040_backup.html>  
   访问日期：2026-07-04  
   定位：parent snapshot、space requirements、snapshot only at successful end。
3. restic official source repository  
   URL: <https://github.com/restic/restic>  
   访问日期：2026-07-04  
   源码定位建议：`internal/repository`（对象保存/索引）与
   `internal/restic`（snapshot/tree 类型）；以当前 tag 为准复核。

#### 官方资料确认的机制

- repository object 以内容 SHA-256 为 Storage ID，只写一次、不就地修改；写入应为原子。
- snapshot 指向 tree，tree 递归引用 data/tree blobs；对象加密且认证。
- 支持多个并行 reader/writer；危险的删除/重写操作需要 exclusive lock，普通操作可用
  non-exclusive lock。
- 官方严格提交顺序：先写包含 data/tree blobs 的 packs，再写引用它们的 indexes，
  最后写 snapshot。只有 snapshot 出现后，reader 才能假定其依赖完整存在。
- 中途空间耗尽会留下未引用数据，但不会创建最终 snapshot，旧 snapshots 仍可用。
- restic 使用 CDC/Rabin，目标平均约 1 MiB；这是成熟实现，但不能证明小型模拟器
  存档一期也需要 CDC。

#### 建议复现实验 / 源码定位

本地临时 repository（只用合成 fixture）：

```bash
rtk proxy env RESTIC_PASSWORD='fixture-only-password' restic init --repo /tmp/mhs-restic
rtk proxy env RESTIC_PASSWORD='fixture-only-password' restic backup --repo /tmp/mhs-restic /tmp/mhs-fixture
rtk proxy env RESTIC_PASSWORD='fixture-only-password' restic snapshots --repo /tmp/mhs-restic
rtk proxy env RESTIC_PASSWORD='fixture-only-password' restic check --read-data --repo /tmp/mhs-restic
```

故障注入：在隔离 backend wrapper 中分别于 pack、index、snapshot 写入后终止进程，
每次运行 `restic check` 并确认只有最后一步成功才出现新 snapshot；旧 snapshot
始终可恢复。

文档顺序检查：

```bash
rtk proxy curl -fLsS 'https://restic.readthedocs.io/en/v0.18.1/design.html' \
  | rtk grep -E 'First, pack files|Then the indexes|finally the corresponding snapshots'
```

#### 优点

- immutable CAS 消除就地更新造成的半写对象。
- lock 和 crash-safe ordering 给出了可直接证明的提交不变量。
- snapshot/tree/blob 分层适合历史、去重、校验和 GC。

#### 事故面

- restic threat model 明确不防存储管理员删除对象；因此仍需 S3 versioning、备份和
  定期完整性检查。
- 密钥泄露后仅改密码不能撤销已泄露 master key；需要新 key/domain 重新封装或迁移。
- prune/GC 是破坏性操作，必须 exclusive lock、mark-and-sweep、grace period。
- CDC 增加实现和兼容成本；一期没有真实收益数据时照搬属于过度设计。

#### 采用 / 不采用

- **采用**：不可变加密 CAS、内容寻址、snapshot DAG、leases/locks、
  `chunks → manifest/index → snapshot → HEAD` 的崩溃安全顺序。
- **一期不采用**：CDC。先用默认 1 MiB fixed chunks，收集插入型写入的重复率和
  CPU/I/O 基准，只有数据证明明显收益时才通过格式版本迁移引入 CDC。
- **不照搬**：restic 的密码/KDF/仓库密钥布局；mh-save-sync 的账户恢复、设备证书、
  域分离和 E2EE 由独立 crypto ADR 定义。

### 3.8 S3 / MinIO + PostgreSQL CAS

#### 官方资料

1. AWS, **Uploading and copying objects using multipart upload in Amazon S3**  
   URL: <https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html>  
   访问日期：2026-07-04  
   定位：init/upload-parts/complete、list parts、checksums、abort。
2. AWS, **Checking object integrity for data uploads in Amazon S3**  
   URL: <https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html>  
   访问日期：2026-07-04  
   定位：full-object/composite checksums、`BadDigest`。
3. AWS, **Retaining multiple versions of objects with S3 Versioning**  
   URL: <https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html>  
   访问日期：2026-07-04  
   定位：保留/恢复每个版本、delete marker。
4. AWS, **Lifecycle configuration to abort multipart uploads**  
   URL: <https://docs.aws.amazon.com/AmazonS3/latest/userguide/lifecycle-configuration-examples.html>  
   访问日期：2026-07-04  
   定位：`AbortIncompleteMultipartUpload`。
5. MinIO, **Bucket Versioning**  
   URL: <https://docs.min.io/aistor/administration/objects-and-versioning/versioning/>  
   访问日期：2026-07-04  
   定位：S3-compatible versions、latest、DeleteMarker、生命周期。
6. MinIO, **mc put**  
   URL: <https://docs.min.io/aistor/reference/cli/mc-put/>  
   访问日期：2026-07-04  
   定位：multipart、part size、checksum。
7. PostgreSQL, **UPDATE**  
   URL: <https://www.postgresql.org/docs/18/sql-update.html>  
   访问日期：2026-07-04  
   定位：boolean `WHERE condition`、`RETURNING`、`UPDATE count`。
8. PostgreSQL, **Explicit Locking**  
   URL: <https://www.postgresql.org/docs/18/explicit-locking.html>  
   访问日期：2026-07-04  
   定位：row-level locks、transaction lifetime、deadlock/retry。

#### 官方资料确认的机制

- S3 multipart 是 init → independently upload parts → complete；可列出已上传 parts，
  因而可恢复中断上传。
- 客户端应记录 part number、ETag/checksum；full-object checksum 不匹配时 S3 以
  `BadDigest` 拒绝对象。ETag 不应被一概当作内容 MD5。
- 未 complete/abort 的 parts 持续占用空间；生命周期规则可自动
  `AbortIncompleteMultipartUpload`。
- S3/MinIO versioning 为相同 object key 保存多个版本，删除默认生成 delete marker；
  它是误删/误覆盖的第二道护栏，但不是逻辑存档 DAG。
- PostgreSQL `UPDATE ... WHERE ... RETURNING` 只返回实际匹配并更新的 row；可把
  `expected_head` 放入 WHERE，实现单语句 compare-and-swap。事务行锁在事务结束释放。

#### 建议复现实验 / 源码定位

MinIO + PostgreSQL 合成 fixture：

```bash
# S3/MinIO：上传一半后中断，ListParts 取得 missing set；恢复并完成后校验 checksum。
rtk proxy mc put --checksum SHA256 /tmp/mhs-object local/mh-save-sync/chunks/fixture
rtk proxy mc stat local/mh-save-sync/chunks/fixture

# 开启版本化并验证覆盖/删除仍可按 version-id 取回。
rtk proxy mc version enable local/mh-save-sync
rtk proxy mc ls --versions local/mh-save-sync/chunks/
```

CAS 的 SQL 形状：

```sql
BEGIN;
INSERT INTO snapshots(id, logical_save_id, manifest_object, parents)
VALUES ($snapshot, $save, $manifest, $parents);

UPDATE logical_save_heads
SET snapshot_id = $snapshot, generation = generation + 1
WHERE logical_save_id = $save
  AND snapshot_id IS NOT DISTINCT FROM $expected_head
RETURNING snapshot_id, generation;
COMMIT;
```

并发实验：

1. A/B 都读取同一 `$expected_head`。
2. 两事务已在对象存储中完成各自 chunks 和 manifest。
3. A 的 CAS 返回 1 row；B 的 CAS 返回 0 rows。
4. B 不删除其 snapshot，而是登记 conflict branch；HEAD 仍指向 A。
5. 删除/损坏 HEAD 所需 manifest 后，readiness/integrity check 必须失败；正常提交路径
   必须在 CAS 前通过 `HEAD`/checksum 验证对象持久化。

#### 优点

- multipart/missing-set 支持大对象断点续传；checksum 提供服务端二次完整性验证。
- versioning/delete marker 降低误覆盖和误删的不可恢复性。
- PostgreSQL 事务适合把 snapshot row、父边、审计和 HEAD CAS 作为一个原子元数据提交。

#### 事故面

- parts 已上传但未 complete 会泄漏存储；必须 lifecycle + 服务端 orphan sweeper。
- complete 的对象可能没有对应 snapshot row（客户端在 DB commit 前崩溃）；它应是
  可安全回收的 orphan，不可被 HEAD 引用。
- 仅检查 HTTP 200 或 ETag 会漏掉 checksum/持久化问题。
- 若把 S3 key 的 latest version 当成逻辑 HEAD，会重新引入 last-write-wins。
- CAS 返回 0 rows 不是可重试覆盖，而是明确分叉；错误重试成无条件 UPDATE 会丢档。
- 长事务或不一致的锁顺序可造成阻塞/死锁；对象上传不得放在数据库事务内等待。

#### 采用 / 不采用

- **采用**：PostgreSQL 作为账号/设备/图/HEAD/上传会话真相源，S3-compatible storage
  保存加密 chunks/manifests/exports。
- **采用顺序**：所有 encrypted chunks durable → encrypted manifest durable →
  PostgreSQL snapshot row/edges → compare-and-swap HEAD。
- **采用护栏**：checksums、missing set、幂等 part/object key、versioning、
  lifecycle abort、orphan grace period、备份/恢复验证。
- **不采用**：S3 object latest、mtime 或数据库 `updated_at` 作为冲突裁决；Redis
  也不作为任何存档真相源。

## 4. 最终组合方案

### 4.1 组合结论

mh-save-sync 固定采用以下成熟机制组合：

1. **Steam 的 session boundary**  
   pre-launch pull/check；save-complete/suspend 可形成高置信候选；正常退出强制
   reconcile；watcher 只标记 dirty。
2. **Switch / PS5 的 per-game capability、状态可见性和手动动作**  
   adapter 明确声明某游戏/slot 能否自动捕获和安全恢复；UI 显示 pending、offline、
   conflict、error；提供立即同步、仅上传、仅下载、历史恢复。
3. **Google / Apple 的 conflict versions**  
   双方 snapshot 都保留；同祖先 fast-forward，父链分叉即 conflict branch；
   二进制存档默认不语义合并。
4. **Syncthing 的 staging / atomic replace**  
   hash 校验后写 staging；恢复前再快照当前状态；模拟器停止后才 replace；无法目录
   原子交换的平台使用 journal + per-file commit + rollback。
5. **restic 的 immutable CAS、locks 和 crash-safe commit ordering**  
   chunk/manifest/snapshot 均不可变；先持久化被引用对象，最后以 PostgreSQL CAS
   更新 HEAD；GC 必须 exclusive lease、mark-and-sweep 与 grace period。

### 4.2 协议不变量

```text
snapshot.parents == expected HEAD:
    CAS 成功 -> fast-forward

snapshot.parents 不含当前 HEAD:
    CAS 失败 -> 保留新 snapshot -> 创建 conflict branch

任意时间：
    HEAD -> snapshot -> encrypted manifest -> encrypted chunks
    上述每条引用都必须指向已持久化且 checksum/AEAD 可验证的对象
```

禁止：

- 静默 last-write-wins；
- 仅按 mtime、设备时钟或 server receive time 裁决；
- watcher 事件直接上传；
- 模拟器运行时把远端内容写回原目录；
- 逐文件从两个分支拼接出从未存在过的“最新目录”；
- HEAD 指向未完成 multipart、缺失 manifest 或缺失 chunk；
- 以对象存储 latest version 代替 PostgreSQL CAS HEAD。

### 4.3 首期取舍

- chunking：默认 1 MiB fixed chunks；CDC 仅在真实 benchmark 证明收益后迁移。
- 冲突粒度：`GameKey + profile + slot` 的完整 snapshot，不是单文件。
- 对象存储 versioning：默认开启作为运维护栏，但客户端历史以 DAG 为准。
- watcher：低成本 dirty hint；稳定窗口、连续指纹、只读 staging copy、manifest/hash、
  adapter validator 全部通过后才能形成 snapshot。
- 自动下载：只进入 local CAS；restore 永远是显式、安全边界内的独立动作。

## 5. 可复现研究检查清单

以下检查只验证“官方资料仍可访问且关键术语仍存在”，不能替代真实设备验收：

```bash
rtk proxy curl -fLsS 'https://partner.steamgames.com/doc/features/cloud?l=english' \
  | rtk grep 'Dynamic Cloud Sync'
rtk proxy curl -fLsS 'https://support.nintendo.com/jp/nso/services/savedata-backup/index.html' \
  | rtk grep 'プレイ中'
rtk proxy curl -fLsS 'https://www.playstation.com/en-ca/support/subscriptions/ps5-ps-plus-cloud-storage/' \
  | rtk grep 'most recent'
rtk proxy curl -fLsS 'https://developer.android.com/games/pgs/android/saved-games' \
  | rtk grep 'Local version'
rtk proxy curl -fLsS 'https://developer.apple.com/documentation/foundation/nsfileversion' \
  | rtk grep 'unresolvedConflictVersions'
rtk proxy curl -fLsS 'https://docs.syncthing.net/users/syncing.html' \
  | rtk grep 'temporary copy'
rtk proxy curl -fLsS 'https://restic.readthedocs.io/en/v0.18.1/design.html' \
  | rtk grep 'finally the corresponding snapshots'
rtk proxy curl -fLsS 'https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html' \
  | rtk grep 'BadDigest'
rtk proxy curl -fLsS 'https://www.postgresql.org/docs/18/sql-update.html' \
  | rtk grep 'RETURNING'
```

当官方页面结构或语义变化时，必须更新本文的访问日期、定位、实验和采用结论；不能只
修链接而保留未经重新验证的旧结论。
