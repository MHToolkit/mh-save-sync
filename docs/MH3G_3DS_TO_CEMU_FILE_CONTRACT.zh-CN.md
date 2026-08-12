# MH3G 3DS 到 Wii U/Cemu 文件契约

[English](MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) | [简体中文](MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md)

本文记录的是**已经实现的 CLI 契约**，不是根据 Cemu 存档目录进行的推测。它只适用于 `mh3g-save-convert` 接受的日版 MH3G `0x2B` profile。

CLI 会检查文件名、字节大小和存档头。它不会根据父目录推断游戏，因此操作者或未来的 UI 必须主动选择正确的日版 MH3G HD Cemu 存档根目录。

## 需要提供的源材料

所有源数据必须来自同一时间点的同一份 3DS/Nemessix/Azahar 存档。不要混用不同时间复制的文件。

| 期望结果 | 需要的 3DS 输入 | 当前 CLI 命令 | 可能发生变化的 Cemu 目标 |
| --- | --- | --- | --- |
| 角色、村/任务/事件状态、农场、狩猎船、玩家数据以及槽位内离线猎人缓存 | 一个明确的散装 `user1`、`user2` 或 `user3` 文件 | `convert <user#> --output <same user#>` | 只会改变同一个 Cemu `user#` 文件 |
| 共享画廊/动画标记 | 散装 3DS `system`，同时提供已存在且已初始化的 Cemu `system` 基线 | `convert-system system --output <已有 Cemu system>` | 只改变 Cemu `system` 中已确认的标记范围；其余目标字节全部保留 |
| 已收到/本地公会名片数据以及离线集会所伙伴对应的名片部分 | 包含下列全部八个文件的完整 3DS ExtData `user` 目录 | `convert-extras --source-dir <extdata user> --output-dir <new empty staging dir>`，随后执行 `install-extras --staging-dir <staging> --target-dir <Cemu save dir> --groups guild-cards` | 先只生成暂存文件；明确执行完整组件组事务后才安装 `card1`、`card2`、`card3`、`cardbox`，并保留 manifest 与原目标字节 |
| 下载或创建的任务 | 同一完整 3DS ExtData `user` 目录 | `convert-extras`，随后执行 `install-extras ... --groups quests` | 先只在暂存目录中生成 `quest1` 到 `quest4`；明确执行完整组件组事务后才安装 |
| 擦身通信/猎人搜索缓存 | 可选的 3DS CEC 邮箱根目录，其中包含 `InBox___/BoxInfo_____` 和收到的消息文件 | 先执行 `convert-cec --dry-run`，随后执行 `convert-cec --write --experimental --expected-source-record-set-sha256 ... --expected-target-sha256 ...` | 只改变 Cemu `cec` 及其事务文件 |

主 README 中给出的常见位置只是示例，不是硬编码路径。逻辑源数据组如下：

```text
3DS 主存档 Savedata (.../title/00040000/00048100/data/00000001/)
  user1 | user2 | user3 | system

3DS 共享 ExtData (.../extdata/00000000/00000481/user/)
  card1  card2  card3  cardbox  quest1  quest2  quest3  quest4

3DS 擦身通信/CEC (.../CEC/00048100/)
  InBox___/BoxInfo_____
  InBox___/_*                  # 收到的 MH3G 消息文件
```

`00000481` 是 MH3G ExtData 根目录。`convert-extras` 所需的 `--source-dir` 是其 `user` 子目录，并且全部八个指定文件都必须是该目录的直接子文件；不能传根目录、`boss` 子目录或手工挑选的不完整文件集。同理，CEC 的 `--source-dir` 是包含 `InBox___` 的 `00048100` 邮箱根目录，不能只传 `InBox___` 子目录。

## 压缩包输入

CLI 只接受普通文件系统中的文件和目录。它不能打开 ZIP、7z、RAR、QQ 或浏览器压缩包预览，也不会从父目录递归发现存档。必须先把压缩包完整解压到普通本地目录，然后选择上文说明的准确 `user#`/`system` 文件、准确 ExtData `user` 目录或准确 CEC `00048100` 目录。包含空格的路径必须加引号。

核心槽位命令要求源文件和目标文件的 basename 完全相同。例如，名为 `user2` 的源文件只能写入名为 `user2` 的目标，不能用于覆盖 `user1` 或任意改名文件。

对于受保护的 `convert` 写入，应绑定本次 Dry Run 的源 SHA-256，再加上一种目标条件。已有目标使用 `--expected-target-sha256`；新输出使用 `--expected-target-absent`，两种条件互斥。`convert-system` 不允许新目标：写入必须同时提交紧邻 Dry Run 的源 SHA-256 和已有目标 SHA-256。事务会取得锁后再次检查条件。

## 准确组件组

### 必需的核心槽位

正常迁移角色时，只提供一个源 `user#` 文件，并选择同编号的 Cemu 目标：

| 源文件 | 接受的源大小 | Cemu 输出 | 输出大小 | 是否必需 |
| --- | ---: | --- | ---: | --- |
| `user1` | `0x8A00` | `user1` | `0x8A24` | 三个槽位中选择一个 |
| `user2` | `0x8A00` | `user2` | `0x8A24` | 三个槽位中选择一个 |
| `user3` | `0x8A00` | `user3` | `0x8A24` | 三个槽位中选择一个 |

选中的 `user#` 是核心转换器唯一必需的输入。它包含主角色状态、槽位内离线猎人名单和候选/缓存数据。`convert` 永远不会自动打开 `system`、`card*`、`quest*`、`cec`、其他 `user#` 或父目录中的其他文件。

### 旧转换存档兼容修复

兼容修复与全新转换是两个独立流程。兼容修复分成五个彼此独立授权的域；选择或完成其中一个域，不会替另一个域授权、写入或回滚：

| 修复域 | 原始 3DS 输入 | 只读当前 Wii U/Cemu 权威数据 | 独立输出 | 命令 |
| --- | --- | --- | --- | --- |
| 核心槽位 | 准确的 `user1`、`user2` 或 `user3` | 同名 `user#` | 同名 `user#` | `repair-converted` |
| 公会名片 | 含 `card1`-`card3`、`cardbox` 的 ExtData `user` 目录 | 含已初始化同名文件的目录 | 含已初始化同名文件的目录 | `repair-extras --group guild-cards` |
| 任务 | 含 `quest1`-`quest4` 的 ExtData `user` 目录 | 含已初始化同名文件的目录 | 含已初始化同名文件的目录 | `repair-extras --group quests` |
| 共享画廊/动画 | 准确的 3DS `system` | 准确且已初始化的 Cemu `system` | 准确的 `system` 路径 | `repair-system` |
| 擦身/CEC 缓存 | 准确的 MH3G CEC 邮箱目录 | 准确且已初始化的 Cemu `cec` | 准确的 `cec` 路径 | `repair-cec` |

每个域都有自己的 Dry Run 指纹、写入授权、事务 manifest 和回滚。原始 3DS、当前 Wii U/Cemu 与输出路径不会在控件之间自动复制或级联。原生界面的核心槽位可选择准确 `user#` 文件或其直接父目录，但不会递归扫描 3DS SD 根目录、Cemu MLC 或压缩包；`system` 和 `cec` 使用准确文件。公会名片与任务在物理上位于同一个目录，因此共用三组 ExtData 目录选择器，但两组仍各自执行 Dry Run、写入、记录 manifest 和回滚。

核心槽位契约为：

```text
mh3g-save-convert repair-converted <3DS-user#> --current <当前-Cemu-user#> \
  --output <修复后-Cemu-user#> \
  [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] \
  [--dry-run | --write --expected-source-set-sha256 <SHA256> \
    --expected-current-set-sha256 <SHA256> --expected-output-set-sha256 <SHA256> \
    --expected-preview-sha256 <SHA256>]
```

原始 3DS 槽位与 `--current` 都是只读合并输入，只有 `--output` 可以写 payload；三者必须为同名槽位。当前 Cemu 数据是继续游玩的权威数据。只有仍等于所选旧转换器输出的已知历史字段才会更新，之后在 Wii U 侧变化的字段会保留并报告为冲突。省略 `--output` 仅保留给 CLI 旧脚本原地写回；macOS/Windows 原生界面始终单独显示并传入输出。

两个 ExtData 域使用：

```text
mh3g-save-convert repair-extras --group <guild-cards|quests> \
  --source-dir <3DS-ExtData-user> --current-dir <当前-Cemu-存档目录> \
  --output-dir <已初始化的输出-Cemu-存档目录> \
  [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] [--dry-run | --write ...]
```

所选输出组必须已经初始化并且完整：公会名片组必须同时包含 `card1`、`card2`、`card3`、`cardbox`；任务组必须同时包含 `quest1`、`quest2`、`quest3`、`quest4`。不允许单文件修复。公会名片字段使用历史三方合并；任务 payload 目前没有经过审查的历史缺陷映射，所以任务修复只校验完整组，并把**当前 Wii U 字节**逐字节写入独立输出，不会恢复成原始 3DS 任务数据。

共享 `system` 修复使用：

```text
mh3g-save-convert repair-system <3DS-system> --current <当前-Cemu-system> \
  --output <修复后-Cemu-system> [--dry-run | --write ...]
```

它只把已确认的画廊/动画位范围并集合并到当前 Cemu 权威数据，其余当前字节（包括其他槽位共享状态）全部保留。输出文件可以尚不存在，但父目录必须存在；该事务与核心槽位和 ExtData 完全独立。

实验性 CEC 修复使用：

```text
mh3g-save-convert repair-cec --source-dir <3DS-MH3G-CEC-邮箱目录> \
  --current <当前-Cemu-cec> --output <修复后-Cemu-cec> \
  [--slot <N>] [--dry-run | --write --experimental ...]
```

它会替换准确识别出的历史记录，保留无关当前槽位，其余记录只填入空槽。CEC 仍是实验性功能，写入必须显式确认；输出可以尚不存在，但当前与输出文件名都必须为 `cec`。

核心、公会名片、任务的 Dry Run 会分别报告 `exact`、`compatible-range`、`ambiguous` 或 `unknown`。`ambiguous` 必须显式选择未被证据否定的 `--from-version` 并重新 Dry Run，`unknown` 会拒绝修复。因为三个域来自同一次历史转换，原生界面会复用同一个手动选择的 0.0.3-0.0.6 版本，但每个域仍独立产生证据与写入授权；`system` 和 CEC 不使用历史转换器版本。

为兼容旧脚本，`repair-converted --source-extdata-dir` 仍可使用，并可通过一个旧式总 manifest 协调核心与公会名片；新的原生界面不再使用该耦合参数。应优先使用独立命令及其对应回滚：核心用 `rollback-repair`，单个 ExtData 组用 `rollback-extras`，`system` 用 `rollback`，CEC 用 `rollback-cec`。`phrase1` 至 `phrase3` 仍不属于修复范围。

### 可选的共享 `system`

`system` 是独立共享组件，不是 `convert user#` 的隐含副作用。

| 源文件 | 接受的源大小 | Cemu 输出 | 输出大小 | 命令 |
| --- | ---: | --- | ---: | --- |
| `system` | `0x3000` | `system` | `0x3024` | `convert-system` |

只有迁移明确包含管家画廊/动画记录时才提供它。`system` 由三个角色槽位共享，还包含不属于当前角色的设置，因此命令必须同时接收 3DS 源文件和已存在、已初始化的 Cemu 目标。它会识别 `0x3000` 的 3DS profile 与 `0x3024` 的 Cemu profile，只对已确认的画廊/动画标记范围（Cemu 文件偏移 `0x68..0x77`）逐位取并集，并保留 Cemu 头和其他全部目标字节。目标不存在或格式错误时会拒绝执行。不运行该事务会保持 Cemu `system` 不变，因此只转换核心槽位不能补齐缺失的画廊记录。

### 可选的共享 ExtData

当前 CLI 的**转换输入**采用一个不可拆分的整组契约。即使最终只安装部分 Cemu 输出，源目录也必须包含下面全部文件：

| 3DS 源文件名 | 源大小 | 生成的 Cemu 文件名 | 生成大小 | 内容组 |
| --- | ---: | --- | ---: | --- |
| `card1` | `0x58000` | `card1` | `0x58024` | 公会名片 |
| `card2` | `0x58000` | `card2` | `0x58024` | 公会名片 |
| `card3` | `0x58000` | `card3` | `0x58024` | 公会名片 |
| `cardbox` | `0x30000` | `cardbox` | `0x30024` | 公会名片存储 |
| `quest1` | `0x29000` | `quest1` | `0x29024` | 下载/创建任务 |
| `quest2` | `0x29000` | `quest2` | `0x29024` | 下载/创建任务 |
| `quest3` | `0x29000` | `quest3` | `0x29024` | 下载/创建任务 |
| `quest4` | `0x29000` | `quest4` | `0x29024` | 下载/创建任务 |

`convert-extras` 会读取全部八个原始文件，并生成全部八个输出。它没有 `--components` 或单文件转换模式。独立的 `install-extras` 可以把经过校验的暂存目录中的一个或两个**完整**组件组安装到已初始化的 Cemu 目标；转换时仍必须收集完整 ExtData 目录，且绝不能单独安装某一个 `card#` 或 `quest#` 文件。

`card1`、`card2`、`card3` 和 `cardbox` 构成支持的公会名片组。三个 `card#` 文件共享完整的已接收名片布局；`cardbox` 使用自己的紧凑布局和转换表。不能把其中任何文件从 3DS 原样复制到 Cemu。

`quest1` 到 `quest4` 是独立任务组。因为输入契约固定，它们由同一暂存命令转换，但并不是公会名片依赖。

`--reset-guild-cards` 不是正常转换。显式传入时，它会创建空白 Cemu `card1`、`card2`、`card3` 和 `cardbox`，丢弃源公会名片数据；任务输出仍按正常方式转换。

### 安装暂存 ExtData

`install-extras` 是唯一支持的暂存 ExtData 覆盖安装路径：

```text
mh3g-save-convert install-extras [--dry-run | --write] \
  --staging-dir <暂存目录> --target-dir <已初始化的-Cemu-存档目录> \
  --groups <guild-cards,quests>
```

暂存目录必须包含全部八个已转换文件。`guild-cards` 始终表示 `card1`、`card2`、`card3` 和 `cardbox`；`quests` 始终表示 `quest1` 到 `quest4`。目标必须是已经初始化的 MH3G Cemu 存档目录，并且包含所选组件的同名文件。Dry Run 会报告暂存组件集与目标组件集的 SHA-256。紧接着的 `--write` 必须通过 `--expected-staging-set-sha256` 与 `--expected-target-set-sha256` 提交这些精确值；写入持有目录锁时会重新检查。它会创建绑定 manifest 的恢复事务，并在替换任何所选组件前保留旧目标字节。

### 可选的擦身通信/CEC

CEC 既不属于 `user#`，也不属于 `card*`，正常公会名片/离线伙伴流程不依赖它。它是独立的实验性缓存导入：

| 3DS 输入 | Cemu 输出 | 写入条件 |
| --- | --- | --- |
| CEC 根目录中 `InBox___/_*` 下收到的非空 MH3G 记录 | `cec` | `convert-cec --write --experimental`，并携带紧接着的 Dry Run 中 `source_record_set_sha256` 和 `target_sha256_before` |

`convert-cec` 要求存在 `InBox___/BoxInfo_____`；它会故意忽略 `OutBox__` 记录，因为这些记录描述源猎人自己发出的广播。它会报告与顺序无关的 `source_record_set_sha256` 和 `target_sha256_before`。写入必须把两项值作为 `--expected-source-record-set-sha256` 和 `--expected-target-sha256` 传回；`cec` 不存在时，目标值表示规范的空 Cemu 容器。写入会取得目标锁、重新读取两边输入并检查两项值后才创建缓存。它不会写入名片文件或 `user#` 文件。

`inspect-cec` 的只读检查范围更广：它会报告 `InBox___` 和 `OutBox__`，还可以选择读取一个 `user#`，用途仅是定位名片 anchor。

## 公会名片和离线集会所伙伴

已支持的文件级依赖为：

```text
编号匹配的 user#
  + card1 + card2 + card3 + cardbox
  = 正常公会名片和离线集会所伙伴迁移组
```

`user#` 保存六条离线猎人名单/缓存记录和候选 anchor。转换器会转换其平台相关字段。公会名片组件保存对应名片 body。回归测试证明 `user#` 中保留的 8 字节 anchor 与转换后名片槽位中的 anchor 匹配。因此，要保留已经收到的名片及其离线集会所伙伴，迁移必须同时保留两侧：选中的 `user#` 和全部四个名片组件。

目前没有证据支持“只选择一个 `card#` 文件也能安全得到该结果”的规则。应把全部四个名片文件视为同一个安装组。反过来，CEC 也不是这些已有名片/伙伴记录的前置依赖；其原始收件记录导入仍明确属于实验性功能。

## 各命令的读写边界

| 命令 | 读取 | 默认/dry-run 时写入 | 使用 `--write` 时写入 |
| --- | --- | --- | --- |
| `inspect <file>` | 一个明确源文件 | 无 | 不适用 |
| `inspect-progress <user#> [--target <user#>]` | 源槽位和可选目标槽位 | 无 | 不适用 |
| `inspect-events <user#> [--target <user#>]` | 源槽位和可选目标槽位 | 无 | 不适用 |
| `convert <user#> --output <same user#>` | 源槽位；只有安装时才读取已有目标和旧事务记录 | 无 | 指定目标槽位及下述核心事务文件 |
| `repair-converted <3DS-user#> --current <当前-Cemu-user#> --output <修复后-Cemu-user#>` | 原始 3DS 槽位、只读当前 Cemu 槽位和独立输出状态 | 无 | 只改变输出侧确认需要修复的同名 `user#` 字段，并创建核心兼容 manifest；当前引用保持不变 |
| `repair-extras --group guild-cards ...` | 完整的原始、当前、输出公会名片组 | 无 | 只改变完整输出 `card1`-`card3`/`cardbox` 组，并创建独立 ExtData 事务 |
| `repair-extras --group quests ...` | 完整的原始、当前、输出任务组 | 无 | 把完整当前 `quest1`-`quest4` 组复制到独立输出并记录自己的事务；绝不恢复原始任务 payload |
| `repair-system system --current system --output system` | 原始 3DS `system`、只读当前 Cemu `system` 和独立输出状态 | 无 | 基于当前 Cemu 字节只写已确认的画廊/动画并集，并创建核心式 manifest |
| `repair-cec --source-dir ... --current cec --output cec` | 3DS CEC 邮箱、只读当前 Cemu `cec` 和独立输出状态 | 无 | 在输出 `cec` 中替换准确历史记录/填充空槽并创建 CEC manifest；要求 `--experimental` |
| `convert-system system --output <已有 Cemu system>` | Dry Run 和写入都会读取 3DS 源 `system` 与已初始化的 Cemu 目标 | 无 | 只把已确认的画廊/动画标记并集合并到指定目标，并创建相同模式的事务文件 |
| `convert-extras --source-dir ... --output-dir ...` | 全部八个 ExtData 文件 | 无，也不会创建输出目录 | 只写入 `output-dir` 下生成的八个文件 |
| `install-extras --staging-dir ... --target-dir ... --groups ...` | 完整暂存 ExtData 集合及被选中、已初始化的目标组件组 | 无 | 只改变被选中的完整 Cemu 组件组，以及下文一个绑定 manifest 的 ExtData 恢复事务 |
| `inspect-cec --source-dir ... [--target cec] [--source-slot user#]` | CEC `InBox___` 和 `OutBox__`；可选 `cec` 和可选用户槽位 | 无 | 不适用 |
| `convert-cec --source-dir ... --target cec` | 收到的 `InBox___` 记录以及已有 `cec`（如存在） | 无 | `cec` 和 CEC 事务文件；要求 `--experimental` 与两项预期 Dry Run 哈希 |
| `rollback` | 受控核心/system manifest、目标和备份 | 不适用 | 只恢复或删除 manifest 绑定的核心/system 目标，并清理其事务文件 |
| `rollback-repair` | 兼容修复总 manifest 及其核心/ExtData 子 manifest | 不适用 | 以受控顺序回滚兼容修复涉及的全部子事务 |
| `rollback-extras` | 受控 ExtData 事务 manifest、被选中目标组件组和保留的旧字节 | 不适用 | 只恢复 manifest 绑定的完整组件组 |
| `rollback-cec` | 受控 CEC manifest、目标和备份 | 不适用 | 只恢复或删除 manifest 绑定的 CEC 目标，并清理其事务文件 |

`convert` 和 `convert-system` 成功写入时会使用同目录临时文件和原子 rename。旧目标存在时会创建按哈希寻址的备份。持久受控文件为：

```text
.<user#|system>.mh3g-backup-<previous-sha256>       # 仅在目标原本存在时
.<user#|system>.mh3g-install.json
.<user#|system>.mh3g-install-history-<sha256>.json # 重复安装时可能出现
```

短暂存在的 `.<user#|system>.mh3g-install.lock` 和临时文件会在事务结束后删除。`convert-extras` 故意不提供目标备份、manifest 或覆盖路径：如果八个同名暂存输出中的任何一个已经存在，`--write` 会拒绝执行。应使用新暂存目录，并对比报告的哈希。

`install-extras` 提供受控安装步骤。它会在目标下创建唯一隐藏的 `.mh3g-extra-transaction-.../` 目录，目录内包含返回的 `.mh3g-extra-recovery.json` manifest 和保留的旧组件字节。 advisory lock 只会在操作期间持有，但常规文件 `.mh3g-extra-install.lock` 会作为稳定 lock inode 持续保留。`rollback-extras` 只接受返回的 manifest，并恢复其中记录的完整组件组；不接受单个组件路径。回滚后事务目录和 manifest 仍作为审计证据保留，但被选中 payload 的每一个字节都会恢复。

实验性 CEC 对应的持久文件名为：

```text
.cec.mh3g-backup-<previous-sha256>  # 仅在 cec 原本存在时
.cec.mh3g-install.json
```

## 不会被自动修改的文件

转换器没有递归“转换整个存档目录”的命令。除非上表中的命令明确指定某条路径，否则不会修改该文件。具体包括：

- 转换 `user2` 不会修改 `user1`、`user3`、`system`、`card1`、`card2`、`card3`、`cardbox`、`quest1` 到 `quest4` 或 `cec`。
- 转换 `system` 不会修改任何 `user#`、`card*`、`quest*` 或 `cec`。
- `convert-extras` 不会修改任何源文件、用户槽位、`system` 或 `cec`；它只写入明确的暂存输出。
- `install-extras` 不会修改源文件、任何 `user#`、`system`、`cec` 或未被选中的 ExtData 组件组；只会改变选中的完整目标组件组及其受控事务文件。
- `convert-cec` 不会修改 `user#`、`system`、`card*` 或 `quest*`。
- 原生独立域流程中的 `repair-converted` 只改变明确输出的 `user#`；唯一耦合例外是为旧 CLI 脚本保留的 `--source-extdata-dir`。
- `repair-extras` 只改变选中的完整输出组。即使公会名片和任务三组目录控件填写了相同路径，两次运行也不会互相授权或回滚。
- `repair-system` 只改变明确输出的 `system`；`repair-cec` 只改变明确输出的 `cec`。
- 任何转换器命令都没有枚举 `phrase1`、`phrase2` 或 `phrase3`，MH3G 转换实现不会读取或写入它们。
- 从该 CLI 的视角看，3DS 源存档文件始终只读。

明确 payload 文件以外的唯一例外，是前文说明的相邻 backup、manifest、history、lock 和临时事务文件。

## 实现证据

本契约来自可执行实现和测试：

- `crates/mh3g-save-convert/src/main.rs`：CLI 参数、同名槽位校验、独立的 `repair-converted`/`repair-extras`/`repair-system`/`repair-cec` 授权与 manifest、八文件 `convert-extras` 循环和 Dry Run 不写入行为。
- `crates/mh3g-save-convert/src/converter.rs`：准确的八个 ExtData 文件名、逐组件校验、公会名片与任务的不同转换行为，以及源只读的纯槽位转换。
- `crates/mh3g-save-convert/src/profile.rs`：接受的 `user1`/`user2`/`user3` 和 `system` basename，以及源/Cemu 字节大小 profile。
- `crates/mh3g-save-convert/src/transaction.rs`：核心/system 原子安装、backup、manifest、history、lock 和 rollback 边界。
- `crates/mh3g-save-convert/src/cec.rs`：仅收件箱的实验性 CEC 导入、修复时准确历史记录替换、`cec` 目标校验以及 CEC backup/manifest 行为。
- `crates/mh3g-save-convert/tests/cli.rs` 和 `crates/mh3g-save-convert/src/converter.rs` 测试：Dry Run 不写入、跨槽位拒绝、独立输出/当前引用保留、完整任务组复制、system 共享字节保留、CEC outbox 拒绝以及离线猎人/名片 anchor 回归覆盖。
