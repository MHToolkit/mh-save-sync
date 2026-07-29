# MH Save Sync

[English](README.md) | [简体中文](README.zh-CN.md)

面向 macOS 与 Android 的跨平台、多模拟器存档同步项目，使用端到端加密快照和可自托管服务。

本仓库目前处于**研究/Alpha 阶段**。在 `docs/ROADMAP.md` 中的数据完整性门槛全部通过前，不应将其视为稳定的备份产品。

## 安全约束

- 本地模拟器存档保持原始格式和原始位置。
- 文件监控只会把存档标记为 dirty，不会直接上传。
- 远端数据不会写入正在运行的模拟器存档目录。
- 并发历史会形成冲突分支，不会静默采用最后写入覆盖。
- 恢复操作会先为当前状态创建快照，然后才替换数据。
- 服务端不会收到恢复密钥或明文存档内容。

## 日版 MH3G 3DS -> Cemu 离线转换

`mh3g-save-convert` 用于把**一个日版 MH3G 3DS 角色槽位**转换到编号相同的日版 MH3G HD Cemu 槽位。它是仅在本地执行的单向工具：不会上传存档、不会修改 3DS 源文件、不支持其他地区版本，也不能把 Cemu 存档反向转换成 3DS 存档。转换器会保留已记录转换范围以外的字节，但字节得到保留并不能证明两个平台中所有游戏字段的含义完全相同。

### 原生 macOS 与 Windows 工作台（开发中）

`apps/mh3g-save-converter-macos` 是独立的前台 SwiftUI App，
`apps/mh3g-save-converter-windows` 是对应的 WinUI App；两者都不是已有的 MH Save
Sync 菜单栏客户端。它们只通过 argv 数组调用随包的 `mh3g-save-convert` 并读取其
JSON 报告；不会在 UI 内重写字节转换、备份、manifest、模拟器进程检查或回滚规则。
macOS 窗口会正常出现于 Dock 和 Cmd-Tab；默认跟随系统语言，也可在设置中切换简体中文或
English。

工作台提供四个**有引导、但不强制**的阶段：输入与检查、可选共享数据、Dry Run、写入或
回滚。完成一个阶段后会显示推荐的下一步操作（例如检查完成后配置可选数据），但玩家仍可
返回前一阶段，或跳过可选组件。该引导不会额外执行任何转换。

核心槽位在 GUI 中可以选择一个准确的 `user1`、`user2`、`user3` 文件，或选择它的直接父
目录；然后只解析用户所选槽位对应的直接子文件。目标可以选择已有且同名的 Cemu `user#`
文件，也可以选择明确的导出目录；选择导出目录只会解析为 `<目录>/user#`，不会在选择时
创建文件。GUI 不会递归扫描、搜索 SD 卡、推断 MLC 根目录，也不接受压缩包。这些目录便利
**只属于 GUI**：CLI 仍要求下文说明的准确源文件与准确输出文件。

可选 ExtData 选择器接受准确的 `user` 目录，也接受常见的直接父目录 `00000481`，并且只解析
该父目录下的 `user` 子目录。CEC 位于独立、默认折叠的实验性页面；正常的公会名片/离线伙伴
组不依赖它。核心角色写入只有在当前 Dry Run 指纹仍与所选源、目标和组件范围一致时才会启用。
已有目标会纳入目标 SHA-256；新的导出目标则记录为“目标不存在”，写入时传入
`--expected-target-absent`。若目标在 Dry Run 后出现，写入会被拒绝，绝不会覆盖新出现的文件。

在 arm64 macOS 开发主机上，可只使用合成 fixture 构建并验证：

```bash
bash scripts/build-mh3g-save-converter-macos-app.sh
bash scripts/mh3g-save-converter-macos-smoke.sh
bash scripts/package-mh3g-save-converter-macos.sh
```

smoke 脚本会创建临时零内容 fixture，并始终验证 inspect、dry-run、包内 diagnostics 和源文件
hash 不变。没有模拟器运行时，它还会验证事务写入和 manifest 绑定回滚；若 Nemessix、Azahar
或 Cemu 已在运行，则会验证 CLI 拒绝合成写入且临时目标未创建。它不会启动 Cemu，也不会打开
真实 MLC。

Windows 10 1809+/Windows 11 x64 的本地 WinUI 发包不要拆成 IDE/Qoder 的多条手工命令。
从仓库根目录运行唯一脚本即可预检 .NET 8、Rust MSVC 和 Visual Studio Build Tools/Windows
SDK，构建并自检 WinUI + Rust sidecar ZIP：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1
```

首次缺少依赖时才显式追加 `-Bootstrap`；它使用 `winget` 安装缺失组件，可能要求管理员批准。
若 WinGet 的 Rustup 安装登记残留、但当前用户缺少 `rustup.exe`，同一条 bootstrap 会原地修复，
并以官方 HTTPS bootstrapper 与其 SHA-256 sidecar 完整性校验兜底，不会删除 `.cargo` 或 `.rustup`。常规运行不会清 NuGet/Cargo 缓存；若旧版脚本曾以 `-NoPath` 把私有 .NET 8 SDK 安装到 `%LOCALAPPDATA%\MH3GSaveConverter\BuildTools\dotnet8\dotnet.exe`，新脚本会直接复用，避免重复下载。产物、SHA-256 与失败诊断位于 `artifacts\`，其中
`mh3g-save-convert-windows-build-transcript.txt` 需回传首个失败命令的完整输出块。完整的
Windows 前置条件、行为和可选参数见
[`apps/mh3g-save-converter-windows/README.zh-CN.md`](apps/mh3g-save-converter-windows/README.zh-CN.md)。

> **使用前必读：**当前 CLI **不能直接读取 ZIP、7z 或 RAR**，也不会在任意存档目录中递归搜索可能的文件。必须先把压缩包完整解压到普通本地目录，然后按照下文要求传入准确的文件或准确层级的目录。上文为 GUI 提供的核心目录选择便利**不适用于 CLI**。不要从 QQ 或浏览器的压缩包预览界面直接运行程序。路径中包含空格时必须加引号。

执行任何 `--write`、`rollback`、`rollback-extras` 或 `rollback-cec` 前，必须完全退出 Nemessix、Azahar 和 Cemu，并等待相应进程结束。`inspect`、`inspect-progress`、`inspect-events`、`inspect-cec` 以及所有 `--dry-run` 操作都只读。

### 选择正确的解压后输入

日版 MH3G 的主存档和共享数据分布在三个不同位置。ExtData 根目录是 `00000481`，但 `convert-extras` 需要的是其下的 `user` 子目录。CEC 是系统 NAND 中的邮箱，不是 SD 卡 ExtData。

| 数据组 | 需要传给 CLI 的准确输入 | **不能**传入 | 是否必需 | 用途/影响的文件 |
| --- | --- | --- | --- | --- |
| 核心角色槽位 | `title/00040000/00048100/data/00000001/` 下一个明确的 `user1`、`user2` 或 `user3` 文件 | 整个 title 目录、全部槽位、ExtData、ZIP | 必需：三选一 | 角色、剧情/任务进度、农场、狩猎船、本地离线猎人数据；只写入同编号且同名的 Cemu `user#` 目标 |
| 共享 system | 同一 title savedata 目录中的一个明确 `system` 文件 | 整个 title 目录、ZIP | 可选 | 共享系统数据；只写入明确指定的 Cemu `system` 目标 |
| 共享 ExtData | 完整的 `extdata/00000000/00000481/user/` 目录，其直接子文件必须包括 `card1`、`card2`、`card3`、`cardbox`、`quest1`、`quest2`、`quest3`、`quest4` | `00000481` 父目录、`boss/`、不完整文件集合、ZIP | 可选 | 将全部八个文件转换到新的暂存目录；再由独立、受保护的 `install-extras` 事务把完整的 `guild-cards`、`quests` 或两者安装到已初始化的 Cemu 目标 |
| 擦身通信/猎人搜索 CEC | 包含 `InBox___` 的准确 `CEC/00048100/` 目录 | SD 卡 ExtData、单独的 `InBox___` 子目录、ZIP | 可选且为实验性功能 | 读取收到的原始擦身消息，只可能写入 Cemu `cec` |

使用 Nemessix 时，请把 `<ID0>` 和 `<ID1>` 替换为 `sdmc/Nintendo 3DS/` 下两层实际的 32 位十六进制目录名；全零 ID 只是本地模拟器中的常见示例：

```text
3DS 主存档 Savedata
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/title/00040000/00048100/data/00000001/
    user1  user2  user3  system

3DS MH3G 共享 ExtData
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/extdata/00000000/00000481/
    user/                         <- convert-extras 需要传入这个目录
      card1 card2 card3 cardbox quest1 quest2 quest3 quest4
    boss/ icon metadata            <- 不属于转换器输入

3DS 系统擦身通信 CEC 邮箱
  .../nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100/
    InBox___/BoxInfo_____ 和 InBox___/_*  <- 收到的消息
    OutBox__/...                           <- 本机猎人的发出广播
```

如果解压后的目录中直接包含 `user2`，使用 CLI 转换核心槽位时必须传入这个**文件**。在原生工作台中，选择该目录并选中 `user2` 时，只会解析其直接子文件 `user2`。如果目录中直接包含 `card1` 到 `quest4`，该目录只能作为 `convert-extras` 输入。如果解压结果外面还有一层包装目录，需要先进入这一层；上述预期文件必须是 CLI 路径或上述受限 GUI 选择的直接子项。

### 写入前：设置路径、检查和 dry-run

以下示例假设已经安装 Rust，并从本仓库根目录运行。先定义一次命令数组；如果使用打包好的二进制文件，请改为 `CLI=("/path/to/mh3g-save-convert")`。

```bash
CLI=(cargo run --quiet -p mh3g-save-convert --)

# 把两个 ID 和 Cemu 用户目录替换为自己机器上的实际解压路径。
N3DS_ROOT="$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/<ID0>/<ID1>"
SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/user2"
SYSTEM_SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/system"
EXTRAS_SOURCE="$N3DS_ROOT/extdata/00000000/00000481/user"
CEC_SOURCE="$HOME/Library/Application Support/Nemessix/nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100"

# 仅为示例：请选择实际包含 user# 文件的 Cemu 账号目录。
CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
TARGET="$CEMU_DIR/user2"
CEMU_CEC="$CEMU_DIR/cec"

"${CLI[@]}" --help
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET"
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET"
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

`inspect` 只接收一个 `user#` 或 `system` 文件，不写入任何内容。`inspect-progress` 接收一个源槽位、可选的 `--target <Cemu-user#>`，以及用于限制输出的可选 `--quest-id <0..65535>`。`inspect-events` 接收一个源槽位、可选的 `--target <Cemu-user#>`，以及用于同时显示未设置事件坐标的可选 `--all`。任务进度解析器按任务 ID 映射，其中包括 payload 偏移 `0x6E5C` 的 16 个完成状态字；事件解析器覆盖偏移 `0x62AE` 的 58 个简单事件字和偏移 `0x668C` 的分类事件表。

`convert` 接收一个源 `user#` 文件和 `--output <同名-user#>`。`user2` 只能写入名为 `user2` 的目标，不能覆盖 `user1` 或任意改名文件。不传 `--write` 时转换保持 dry-run；在脚本中建议显式传入 `--dry-run`，以便清楚表达只读意图。`--write` 与 `--dry-run` 互斥。

对于 GUI 和自动化调用，`convert` 与 `convert-system` 提供受保护的写入前置条件：`--expected-source-sha256`，再加上二选一的目标条件 `--expected-target-sha256` 或 `--expected-target-absent`。它们都只能与 `--write` 一起使用。哈希值只能取自**同一次**、针对相同源文件和输出路径的紧邻 dry-run JSON 中的 `hashes.source` 与 `hashes.target_before`。这样已有的源或目标在 dry-run 后发生变化时，写入会失败关闭；目标哈希会在取得单槽位安装锁后再次检查。不要复用旧报告，也不要单独计算替代值。

仅当目标文件已存在时，JSON 才会提供 `hashes.target_before`。如果它不存在，不要伪造哨兵哈希或传入 `--expected-target-sha256`。对于受保护的新导出，请传入本次 Dry Run 的源哈希和 `--expected-target-absent`。事务在取得锁后会再次检查；若新目标在此期间出现，则拒绝写入。两种目标条件互斥。

### 完整命令参考

下面所有命令都使用前文定义的 `CLI` 数组。如果不从源码构建，请替换为打包好的二进制文件。

#### `inspect`：读取一个文件

```text
mh3g-save-convert inspect <SOURCE>
```

`<SOURCE>` 是一个能够识别的日版 3DS 或 Cemu `user1`/`user2`/`user3` 或 `system` 文件。它会检查 3DS `0x2B` profile 或 Cemu 容器，输出 profile、大小和哈希信息，不写入任何内容。它也可用于回读转换后的输出：

```bash
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect "$SYSTEM_SOURCE"
```

#### `inspect-progress`：读取任务完成状态

```text
mh3g-save-convert inspect-progress [--target <TARGET>] [--quest-id <QUEST_ID>] <SOURCE>
```

`<SOURCE>` 是一个 3DS `user#`；`--target` 是可选的同槽位 Cemu `user#`；`--quest-id` 按一个数字任务 ID 过滤输出。该命令不写入：

```bash
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET" --quest-id 201
```

#### `inspect-events`：读取剧情/事件标记

```text
mh3g-save-convert inspect-events [--target <TARGET>] [--all] <SOURCE>
```

`<SOURCE>` 和可选 `--target` 的文件类型与上文相同。`--all` 会增加未设置的坐标；不传时主要显示已激活值。该命令不写入：

```bash
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET" --all
```

#### `convert`：转换一个角色槽位

```text
mh3g-save-convert convert [--dry-run | --write [--expected-source-sha256 <SHA256>] [--expected-target-sha256 <SHA256> | --expected-target-absent]] --output <OUTPUT> <SOURCE>
```

`<SOURCE>` 与 `<OUTPUT>` 必须拥有相同的 `user#` 文件名。只读执行：

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

完全停止所有模拟器后，执行原子安装：

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --write
```

面向已有目标文件的 GUI/自动化写入，应保留一份 dry-run JSON，并将其中哈希作为 argv 值传入。下面的 Bash 示例依赖 `jq`，不使用 `eval`，也不会由 JSON 重建 shell 命令：

```bash
set -euo pipefail

DRY_RUN_JSON=$("${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run)
SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$DRY_RUN_JSON")
TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$DRY_RUN_JSON")

"${CLI[@]}" convert "$SOURCE" --output "$TARGET" \
  --expected-source-sha256 "$SOURCE_SHA256" \
  --expected-target-sha256 "$TARGET_SHA256" \
  --write
```

当目标不存在或报告不包含这两个哈希时，`jq -e` 会使这个**已有目标**的受保护流程停止。两个 `--expected-...` 参数不能用于 dry-run，也不能脱离 `--write` 单独传入。

对于受保护的首次导出，目标必须在该次 Dry Run 时不存在。请绑定源哈希和“不存在”条件，而不是伪造目标哈希：

```bash
EXPORT_DIR="$HOME/Downloads/mh3g-cemu-export"
TARGET="$EXPORT_DIR/user2"
NEW_DRY_RUN_JSON=$("${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run)
NEW_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$NEW_DRY_RUN_JSON")

"${CLI[@]}" convert "$SOURCE" --output "$TARGET" \
  --expected-source-sha256 "$NEW_SOURCE_SHA256" \
  --expected-target-absent \
  --write
```

不要在 Dry Run 与写入之间自行创建 `"$TARGET"`。若其他进程创建了它，事务会刻意拒绝写入，
而不是覆盖一个刚出现的存档。

如果目标原本存在，`--write` 会在同目录创建 `.user2.mh3g-backup-<previous-sha256>` 和 `.user2.mh3g-install.json`；重复安装还可能生成 `.user2.mh3g-install-history-<sha256>.json`。在 Cemu 中手动验证成功前，请保留 manifest。

#### `convert-system`：转换共享 system 数据

```text
mh3g-save-convert convert-system [--dry-run | --write [--expected-source-sha256 <SHA256>] [--expected-target-sha256 <SHA256> | --expected-target-absent]] --output <OUTPUT> <SOURCE>
```

只能使用明确的 `system` 文件；它不会读取 `user#` 或 ExtData：

```bash
"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --dry-run
"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --write
```

它使用相同的事务备份/manifest 机制，文件名改为 `.system...`。`--write` 与 `--dry-run` 互斥。相同的受保护写入流程也适用，但必须使用该次 `convert-system` dry-run 的哈希，不能复用角色槽位转换的结果：

```bash
SYSTEM_TARGET="$CEMU_DIR/system"
SYSTEM_DRY_RUN_JSON=$("${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" --dry-run)
SYSTEM_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$SYSTEM_DRY_RUN_JSON")
SYSTEM_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$SYSTEM_DRY_RUN_JSON")

"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" \
  --expected-source-sha256 "$SYSTEM_SOURCE_SHA256" \
  --expected-target-sha256 "$SYSTEM_TARGET_SHA256" \
  --write
```

新的 `system` 导出也按相同规则：使用该次紧邻 Dry Run 的源哈希与
`--expected-target-absent`。它和 `--expected-target-sha256` 互斥；如果目标在写入
取得锁前出现，写入会被拒绝。

#### `convert-extras`：生成共享 ExtData 暂存文件

```text
mh3g-save-convert convert-extras [--dry-run | --write] [--reset-guild-cards] \
  --source-dir <EXTDATA-USER-DIR> --output-dir <NEW-STAGING-DIR>
```

`--source-dir` 必须是完整的 `.../extdata/00000000/00000481/user/` 目录；即使后续只安装名片文件，转换输入仍必须包含全部八个文件。`--output-dir` 是新的暂存目录。如果其中已经存在同名组件，命令会拒绝执行，因此该命令不会直接覆盖 Cemu 存档：

```bash
EXTRAS_OUTPUT="$HOME/Desktop/mh3g-cemu-extras"
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write
```

`quest1` 到 `quest4` 会增加 Cemu 容器。`card1` 到 `card3` 和 `cardbox` 会先应用已恢复的跨平台字段映射，然后写入 wrapper。`--reset-guild-cards` 是明确的破坏性恢复开关：它会生成空白原生 Cemu `card*` 文件，并丢弃本地和已收到的名片数据。正常迁移不要使用它。不要手工把单个生成文件复制进 Cemu；请使用下方受保护的完整组件组安装命令。

#### `install-extras`：事务安装暂存 ExtData 组件组

```text
mh3g-save-convert install-extras [--dry-run | --write] \
  --staging-dir <暂存目录> --target-dir <已初始化的-Cemu-存档目录> \
  --groups <guild-cards,quests>
```

它故意与 `convert-extras` 分离。`--staging-dir` 必须包含全部八个生成文件，而 `--groups`
只能选择完整组件组：`guild-cards` 表示 `card1`、`card2`、`card3` 和 `cardbox`；`quests`
表示 `quest1` 到 `quest4`。目标必须是已初始化的 MH3G Cemu 存档目录，并且已经包含被选择的
同名组件。写入会创建绑定 manifest 的恢复事务并保留目标原始字节；不会单独安装某一个 `card#`
或 `quest#` 文件。

> **Windows 限制：**Windows 支持 `convert-extras` 生成暂存文件和
> `install-extras --dry-run` 预览，但会在尚未改动任何 ExtData 文件前，主动拒绝
> `install-extras --write` 与 `rollback-extras`。安全的多文件安装需要当前转换器完整的持久目录元数据协议和
> 双名称原子交换；请在受支持的平台完成该受保护的安装/回滚步骤。核心 `user#` 与共享 `system`
> 转换仍可在 Windows 使用。

安装前应紧接着执行 Dry Run，并把两组报告哈希绑定到写入：

```bash
EXTRAS_INSTALL_DRY_RUN_JSON=$("${CLI[@]}" install-extras \
  --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
  --groups guild-cards,quests --dry-run)
EXTRAS_STAGING_SHA256=$(jq -er '.staging_set_sha256' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")
EXTRAS_TARGET_SHA256=$(jq -er '.target_set_sha256_before' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")

"${CLI[@]}" install-extras \
  --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
  --groups guild-cards,quests \
  --expected-staging-set-sha256 "$EXTRAS_STAGING_SHA256" \
  --expected-target-set-sha256 "$EXTRAS_TARGET_SHA256" \
  --write
```

#### `inspect-cec`：读取擦身通信/猎人搜索邮箱

```text
mh3g-save-convert inspect-cec --source-dir <CEC-DIR> [--target <CEMU-CEC>] \
  [--source-slot <USER-SLOT>]
```

`--source-dir` 是 CEC `.../CEC/00048100/` 目录，不是 ExtData。`--target` 可选读取一个 Cemu `cec` 文件。`--source-slot` 可选读取一个 `user#`，用途仅是定位其公会名片 anchor。该命令不写入：

```bash
"${CLI[@]}" inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEMU_CEC"
```

#### `convert-cec`：实验性导入收到的消息

```text
mh3g-save-convert convert-cec --source-dir <CEC-DIR> --target <CEMU-CEC> \
  [--slot <SLOT>] --dry-run

mh3g-save-convert convert-cec --source-dir <CEC-DIR> --target <CEMU-CEC> \
  [--slot <SLOT>] --write --experimental \
  --expected-source-record-set-sha256 <SHA-256> \
  --expected-target-sha256 <SHA-256>
```

CEC 既不是主存档，也不是持久公会名片仓库。`InBox___/_*` 是收到的原始消息；`OutBox__/_*` 是本机猎人的发出广播，会被故意忽略。`BoxInfo_____` 是邮箱元数据。只有非空收件箱记录会成为候选导入。已有公会名片和离线集会所伙伴使用下面这组持久数据：

```text
编号匹配的 user# + card1 + card2 + card3 + cardbox
```

即使持久名片列表非空，CEC 收件箱为空也完全正常。`convert-cec` 是独立的**实验性**功能：目前拥有文件级证据，但不代表所有 Wii U UI 都已获得完整运行时保证。它不会写入任何 `user#`、`system`、`card*` 或 `quest*`。默认使用第一个空 Cemu 槽位；`--slot <SLOT>` 用于指定第一个候选槽位，已有非空记录永远不会被覆盖。

CEC Dry Run 会报告与记录顺序无关的 `source_record_set_sha256`，以及
`target_sha256_before`（`cec` 尚不存在时，它表示规范的空 Cemu 容器）。写入必须带上
两项值；工具会在取得 CEC 目标锁后重新检查：

```bash
CEC_DRY_RUN_JSON=$("${CLI[@]}" convert-cec \
  --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --dry-run)
CEC_SOURCE_RECORD_SET_SHA256=$(jq -er '.source_record_set_sha256' <<<"$CEC_DRY_RUN_JSON")
CEC_TARGET_SHA256=$(jq -er '.target_sha256_before' <<<"$CEC_DRY_RUN_JSON")

"${CLI[@]}" convert-cec --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --slot 0 \
  --expected-source-record-set-sha256 "$CEC_SOURCE_RECORD_SET_SHA256" \
  --expected-target-sha256 "$CEC_TARGET_SHA256" \
  --write --experimental
```

3DS 源 wrapper 和观察到的 8 字节消息前缀不会被复制。成功写入 CEC 后，必要时会生成 `.cec.mh3g-backup-<previous-sha256>`，并在目标旁生成 `.cec.mh3g-install.json`。

#### `rollback`、`rollback-extras` 与 `rollback-cec`：只恢复已知事务

```text
mh3g-save-convert rollback --manifest <MANIFEST>
mh3g-save-convert rollback-extras --manifest <EXTDATA-MANIFEST>
mh3g-save-convert rollback-cec --manifest <MANIFEST>
```

两个命令都要求传入转换器生成的准确 manifest，不能传入存档目录、备份文件或压缩包。完全停止所有模拟器后执行：

```bash
"${CLI[@]}" rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"
"${CLI[@]}" rollback-extras --manifest "$EXTRAS_MANIFEST"
"${CLI[@]}" rollback-cec --manifest "$CEMU_DIR/.cec.mh3g-install.json"
```

回滚只会恢复 manifest 绑定的核心目标、ExtData 组件组或 CEC 目标，不会修改 3DS 源文件。请保留每次成功写入输出的 `manifest` 路径；`install-extras` 的 manifest 会在写入 JSON 中返回，并不是固定文件名。

### 运行证据和安装包

转换器目前只识别日版 `0x2B` profile。静态表和来源记录位于 `crates/mh3g-save-convert/data/catalog-provenance.json`；更详细的数据边界参见 [`docs/adr/0013-mh3g-cross-format-conversion.md`](docs/adr/0013-mh3g-cross-format-conversion.md) 和[准确的 MH3G 文件契约](docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md)。

**macOS arm64（已于 2026-07-26 完成隔离 CLI 验证）：**打包后的 release 二进制通过包内校验和、ZIP 校验和、解压以及解压后 `--help` 检查。两个真实日版源槽位（`user1` 和 `user2`）分别通过 `inspect`、`inspect-progress`、`inspect-events`、不创建目标的显式 dry-run、隔离写入、输出回读、重复安装/backup/history 生成和 manifest 绑定回滚。两个源文件均保持逐字节不变；输出大小均为 `0x8A24`，写入哈希与 dry-run 哈希一致。真实八文件 ExtData 目录同样通过了不创建输出目录的 dry-run，并成功写入全新的暂存目录，全部输出大小和哈希均符合报告。CEC 只读检查显示收件箱消息为 0、当前猎人的 outbox 消息为 1，因此没有执行实验性 CEC 写入。本次没有启动 Cemu，也没有读取或写入现有 MLC；这是 CLI/文件级验证，不是新增的游戏内运行时结论。

在 Apple Silicon Mac 上复现同一安装包：

```bash
./scripts/package-mh3g-macos.sh artifacts/mh3g-converter
```

该命令会生成 `mh3g-save-convert-macos-arm64.zip`、对应 `.zip.sha256` sidecar，以及包含二进制、双语 README 和包内二进制校验和的解压暂存目录。打包脚本不会读取任何存档。

**Windows x64：**`.github/workflows/mh3g-converter-windows.yml` 会构建原生静态链接的 `x86_64-pc-windows-msvc` 可执行文件，打包校验和及启动器，模拟 Mark-of-the-Web，并执行合成存档写入/回滚证明。当前 PR 的 GitHub-hosted workflow 结果才是安装包 CI 证据；它不能证明某台具体电脑上的 AppLocker、Smart App Control、杀毒软件、压缩包预览或目录权限策略。只能下载成功 workflow 生成的 artifact，先核对 ZIP SHA-256 sidecar，再完整解压，然后使用包内 `Run-Converter.ps1`。如果 Windows 返回 error 5（`Access is denied`），请保留包含操作名和路径的完整错误行。

## 办公室 Mac ↔ 家中 Android 使用流程

第一阶段 Alpha 使用中文优先的同步工作台，让用户看到实际同步目标，而不是只按一个含义不明的“同步”按钮：

1. 运行或自行托管服务器，并在两台设备上使用同一个 URL。
   - macOS：运行一次 `swift run --package-path apps/macos MHSaveSyncMac --set-server-url <url>`，或者为单次 CLI 会话设置 `MH_SAVE_SYNC_SERVER_URL`。
   - Android：在应用内填写相同服务器地址。
   - 当前隔离 Alpha 测试 API：`http://8.130.112.207:39082`（仅为服务端 API；MinIO/管理端口不是客户端地址）。
2. macOS Nemessix 启动前：先用 `./scripts/install-macos-app.sh` 安装本地菜单栏应用，再打开 `/Applications/MH Save Sync.app`。这是菜单栏工具，应在右上角查找 `MH 云存档`，不会显示在 Dock。首先配置 `设置服务器地址…`、`选择 Mac Nemessix 存档目录…`，并选择 `生成恢复密钥文件`（推荐）或 `选择恢复密钥文件…`。菜单栏标题会在 `MH 云存档 · 设服务器/选目录/选密钥/就绪` 之间变化，菜单顶部始终显示 `同步路线` 和 `下一步：...`。只设置服务器 URL 后，应用仍会要求选择存档目录并生成/选择恢复密钥文件。然后在启动 MH3G 前使用 `启动前检查`，手动同步时使用 `立即上传 Mac 存档到服务器`，退出游戏后使用 `我已退出 MH3G：立即对账上传`，用 `查看云端状态` 查看存档位置；需要自动检测退出时启用 `自动同步：退出 Nemessix 后上传`。应用内 `新手引导：办公室 Mac ↔ 回家 Android` 说明了同一流程。
3. Android Nemessix 启动前：授权 Nemessix 存档目录，保持 `MH3G / Android Nemessix` 启用，然后点击 `启动前检查`。Android 会显示正在上传、正在下载到手机缓存、等待退出 MH3G，或因未设置服务器地址而阻止同步。
4. 云端与本地版本不同时，应用要求明确选择：`云端覆盖本地（先备份，需停止 Nemessix）` 或 `本地替换云端（保留云端旧版本）`。两个历史都会保留，不存在按最新时间静默覆盖。
5. 服务端不可用时，是否继续本地游玩会明确显示。本地队列保持不变，服务器恢复后继续上传。

面向玩家的中文指南：`docs/ux/USER_GUIDE_ZH.md`。
工程 UX 契约：`docs/ux/SYNC_USER_FLOWS.md`。
UI/UX 研究基线：`docs/research/UI_UX_PATTERNS.md`。

## 仓库结构

- `crates/`：共享 Rust 领域模型、引擎、加密、适配器、客户端、服务端和 CLI
- `apps/`：原生 macOS 和 Android 外壳
- `deploy/compose/`：自托管 PostgreSQL 和 S3 兼容部署
- `docs/research/`：有证据支持的研究与实验
- `docs/adr/`：已接受的架构决策
- `docs/api/openapi-v1.yaml`：REST/OpenAPI v1 契约草案
- `scripts/`：可复现的开发、备份和验证工具

## 当前状态

第一阶段功能目前包括：

- 有证据支持的云存档、加密/冲突、模拟器矩阵、写入时间线和自托管研究草案；
- 用于领域、加密、引擎、适配器、客户端、服务端和 CLI 的 Rust workspace；
- 加密固定分块 fixture 快照、DAG 冲突保留、安全恢复门控和 SQLite WAL 元数据测试；
- 使用签名设备证书的 PostgreSQL + S3/MinIO 持久服务、missing-set 断点续传、校验和失败即关闭写入、事务 CAS HEAD 提交、S3 SHA-256 上传校验、bucket 版本初始化和已提交对象引用的 readiness 检查；
- macOS Swift 外壳冒烟、Android SAF/WorkManager/前台服务外壳，以及生成的 UniFFI Kotlin/Swift bridge 证据；
- `cargo deny`、`cargo audit`、依赖审查、CycloneDX SBOM、密钥扫描和 artifact 校验和等 CI 供应链门槛。兼容自托管 runner 的 PR 路径当前会自动运行 Rust 和 Android。MHToolkit 目前只有一个 2c4g `ci-general` runner，因此 CI 会取消旧 push，并串行执行较重的 Rust → Android 任务，避免抢占同一主机。独立的每周 `ci-canary` 只运行轻量 runner/脚本/UX 文案健康检查。自托管 Compose 的 PostgreSQL/MinIO/server 健康检查默认间隔为 15s/15s/30s，小型主机可通过 `MH_SAVE_SYNC_*_HEALTH_*` 环境变量调整。`deploy/compose/compose.tls.yaml` 提供可选 Caddy 反向代理，使生产部署能把 API 保留在 loopback，只对外发布 80/443；macOS 和 Compose 证据记录在 `docs/runbooks/PHASE1_VALIDATION.md`。

仍未稳定：真实 macOS ↔ Android ↔ 第二模拟器往返、完善的导出/导入 UX、升级/回滚基准、受公共信任的 TLS 端点验证以及真实模拟器 bundle 恢复，这些仍是 `docs/ROADMAP.md` 中的开放门槛。无服务器 fixture bundle 恢复由 `scripts/offline-bundle-e2e.sh` 覆盖；隔离的 `mh-save-sync-aliyun` 部署、公共 Alpha API 门槛、灾难恢复门槛和可选 Caddy TLS 反向代理配置门槛记录在 `docs/runbooks/PHASE1_VALIDATION.md`。

## 五分钟本地演示

```bash
cargo test --workspace
cargo run -p save-cli --bin mh-save -- adapters
cargo run -p save-cli --bin mh-save -- crypto-vector
cargo run -p save-cli --bin mh-save -- crypto-device-fixture
cargo run -p save-cli --bin mh-save -- snapshot-fixture tests/fixtures/generic-save
./scripts/offline-bundle-e2e.sh
./scripts/supply-chain-gate.sh
./scripts/automation-policy-e2e.sh
swift run --package-path apps/macos MHSaveSyncMac
swift run --package-path apps/macos MHSaveSyncMac --set-server-url http://127.0.0.1:18080
./scripts/build-macos-app-bundle.sh
MH_SAVE_SYNC_INSTALL_DIR="$PWD/artifacts/local-apps" ./scripts/install-macos-app.sh
./scripts/macos-shell-e2e.sh
./scripts/macos-config-e2e.sh
./scripts/macos-install-e2e.sh
```

在这台 Mac 上安装可双击使用的 macOS Alpha 应用：

```bash
./scripts/install-macos-app.sh
open -a "/Applications/MH Save Sync.app"
```

应用菜单可以直接设置服务器、存档目录、恢复密钥文件、手动上传、云端状态、恢复和退出后自动上传。如果更喜欢脚本，下面的 CLI 路径会写入相同持久配置：

```bash
"/Applications/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac" \
  --set-server-url http://8.130.112.207:39082
```

macOS 外壳可以调用 Android/CLI 演示使用的同一 Rust CLI 流水线：

```bash
export MH_SAVE_SYNC_SERVER_URL=http://127.0.0.1:18080
export MH_SAVE_SYNC_CLI="$PWD/target/debug/mh-save"

swift run --package-path apps/macos MHSaveSyncMac --set-server-url "$MH_SAVE_SYNC_SERVER_URL"

swift run --package-path apps/macos MHSaveSyncMac --server-upload \
  --root tests/fixtures/generic-save \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555

swift run --package-path apps/macos MHSaveSyncMac --server-status \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555

swift run --package-path apps/macos MHSaveSyncMac --server-restore \
  --target /tmp/mh-save-sync-restored \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555 \
  --emulator-state stopped
```

可见的服务端同步演示（显示快照目标、逻辑存档 ID、云端 HEAD 和冲突分支数量）：

```bash
MH_SAVE_SYNC_BIND=127.0.0.1:18080 cargo run -p save-server --bin mh-save-server

cargo run -p save-cli --bin mh-save -- server-upload \
  --server-url http://127.0.0.1:18080 \
  --root tests/fixtures/generic-save \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555 \
  --device-id office-mac

cargo run -p save-cli --bin mh-save -- server-status \
  --server-url http://127.0.0.1:18080 \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555
```

`server-upload` 会输出中文 `message_zh`、`server_url`、`sync_target`、`logical_save_id`、`cloud_head_before`、`cloud_head`、`outcome` 和 `conflict_snapshot`，因此手动同步不是黑盒。客户端能用恢复密钥解密当前 HEAD 和冲突分支时，`server-status` 还会返回 `conflict_diffs`。第一阶段公开一个明确的游戏专用解析器契约：`mh3g-3ds` 目前只报告 MH3G/3U 3DS 存档的文件/字节级差异；在游戏专用解析器证明猎人、装备、道具或任务字段前，项目明确**不会**声称能语义合并这些字段。

本地存档差异冒烟：

```bash
cargo run -p save-cli --bin mh-save -- save-diff \
  --left /tmp/mh-save-left \
  --right /tmp/mh-save-right \
  --game-profile mh3g-3ds
```

可复现门槛是 `./scripts/server-sync-e2e.sh`：它上传办公室快照，再上传一个没有 base head 的家中/Android 风格分叉，并验证云端 HEAD 保持不变、冲突分支得到保留，同时返回用户可读的文件/字节差异元数据。

自动化策略门槛：

```bash
./scripts/automation-policy-e2e.sh
```

它固定 macOS 和 Android 共享的触发契约：文件系统事件只标记 dirty；save-complete、模拟器退出、周期对账和手动同步可以创建稳定快照候选；模拟器运行时仍会阻止远端恢复。

macOS 没有系统 Java 时，使用 Android Studio 自带 JBR 执行 Android 本地构建/lint：

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug
```

在恰好一个已连接 ADB 设备或模拟器上安装/启动生成的 debug APK：

```bash
./scripts/android-apk-smoke.sh
```

该冒烟测试会安装 `apps/android/app/build/outputs/apk/debug/app-debug.apk`，启动 `org.mhtoolkit.savesync/.MainActivity`，检查它是否成为 resumed activity，并在启动 logcat 包含应用崩溃时失败。它不会测试 SAF 或真实模拟器存档访问；在回家执行下面的共享目录同步 E2E 前，可用它快速确认 APK 能否安装。

同一已启动 APK 的 Android UI 文案冒烟：

```bash
./scripts/android-ui-copy-smoke.sh
```

它会导出实际 Android view hierarchy，并要求可见中文文案说明 `MH 云存档同步`、办公室 Mac ↔ 家中 Android、同步路线、禁止静默覆盖、服务器目标、Android Nemessix 目录授权、MH3G 开关和启动前检查。

在已连接 ADB 设备上运行 Android Generic Folder 共享存储冒烟：

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 \
  ./scripts/android-generic-folder-e2e.sh
```

它验证 macOS、公共 Alpha API 和 Android `/sdcard` 共享存储之间的通用用户选定目录路径，包括冲突保留和运行中恢复失败即关闭。它不代表 Nemessix/Azahar/Citra 已通过运行时验证；模拟器专用适配器仍需证明恢复结果能被模拟器读取。

无服务器离线恢复演示：

```bash
./scripts/offline-bundle-e2e.sh
```

它会把 `tests/fixtures/generic-save` 导出为加密 `.mhsavebundle`，恢复到新目录，逐字节比较结果，并验证 `--emulator-state running` 会在不写入目标的情况下失败。参见 `docs/runbooks/OFFLINE_BUNDLE_RECOVERY.md`。

使用外部密钥文件的自托管本地演示：

```bash
secret_dir="$HOME/Documents/Secrets/mh-save-sync-compose"
mkdir -p "$secret_dir"
openssl rand -hex 32 > "$secret_dir/postgres_password.txt"
printf 'mh-save-sync-local' > "$secret_dir/minio_root_user.txt"
openssl rand -hex 32 > "$secret_dir/minio_root_password.txt"
chmod 600 "$secret_dir"/*.txt
printf 'MH_SAVE_SYNC_SECRETS_DIR="%s"\n' "$secret_dir" \
  > "$HOME/Documents/Secrets/mh-save-sync.env"
chmod 600 "$HOME/Documents/Secrets/mh-save-sync.env"

podman compose --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml up -d --build --wait
curl -fsS http://127.0.0.1:18080/ready
python3 scripts/compose-e2e.py
python3 scripts/compose-resume-e2e.py prepare artifacts/compose-resume-state.json
podman compose --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml restart server
python3 scripts/compose-resume-e2e.py finish artifacts/compose-resume-state.json

# 完整持久后端 CLI 恢复门槛。它会在空闲 localhost 端口启动隔离 Compose，
# 上传办公室/家中分叉快照，保持云端 HEAD 不变，逐字节恢复云端 HEAD，
# 并验证模拟器运行时的恢复会失败。
CONTAINER_RUNTIME=podman ./scripts/compose-server-sync-e2e.sh
```

`scripts/compose-server-sync-e2e.sh` 也可通过 `MH_SAVE_SYNC_SERVER_URL=...` 指向已经运行的持久服务。它启动 Compose 时会检查所选容器运行时 daemon 是否可用；如果安装了 Docker CLI 但 daemon 未运行，会切换到 Podman，不会在测试中途失败。轻量运行时选择门槛为 `./scripts/compose-server-sync-e2e-runtime-test.sh`。

备份和破坏性恢复：

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/backup.sh

CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/restore.sh "$HOME/Games/Backups/MHSaveSync/<run-id>"
```

最新本地验证证据汇总在 `docs/runbooks/PHASE1_VALIDATION.md`。
