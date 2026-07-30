MH3G 日版 3DS -> Wii U/Cemu 存档转换器（Windows x64）
MH3G Japanese 3DS -> Wii U/Cemu Save Converter (Windows x64)

==================== 简体中文 ====================

适用范围
--------

- 仅支持日版 MH3G 3DS 存档（0x2B profile）转换到日版 MH3G HD Cemu。
- 这是本地、单向转换；不会上传或修改 3DS 源文件，不能从 Cemu 转回 3DS。
- 不支持直接读取 ZIP、7z、RAR，也不会自动扫描整个存档目录。
- 若选择 ZIP，必须先完整解压到普通本地目录；不要在 QQ/浏览器压缩包预览中运行 EXE。安装版和单文件便携版可直接从普通本地目录运行。
- 执行任何受支持的 `--write`、`rollback` 或 `rollback-cec` 前，必须完全退出 Cemu、Azahar 和 Nemessix。

发布格式（Windows x64）
-----------------------

每次运行仓库根目录的 `scripts\package-mh3g-save-converter-windows.ps1` 都会从同一份 WinUI 发布目录与同一份 Rust CLI sidecar 生成以下三种产物，并各自附带 `.sha256`：

1. `mh3g-save-convert-windows-x64.zip`：传统便携文件夹。完整解压后运行其中的 `MH3GSaveConverter.exe`；文件夹内保留 `tools\mh3g-save-convert.exe` 和 `Run-Converter.ps1`。
2. `MH3GSaveConverter-Setup-x64.exe`：每用户安装器。运行后安装到当前用户的 Programs 目录，可创建开始菜单/可选桌面快捷方式；不需要管理员权限。
3. `MH3GSaveConverter-Portable-x64.exe`：单文件便携 UI。可直接双击，不需要安装，也不需要另行放置 sidecar。首次启动会把 .NET/WinUI 及已打包的 Rust sidecar 解压至用户临时运行缓存；这是单文件分发，不是“零解压”。

三种形式都只支持 Windows x64。请在普通本地目录运行，不要从 QQ/浏览器压缩包预览中打开；下载后先按对应 `.sha256` 校验。单文件便携 UI 不包含 `Run-Converter.ps1`，需要命令行模式请使用 ZIP 或安装版中的完整目录。

先校验下载的 ZIP（在 ZIP 和 .sha256 所在目录执行）
--------------------------------------------------

  $expected = ((Get-Content .\mh3g-save-convert-windows-x64.zip.sha256 -Raw).Trim() -split '\s+')[0]
  $actual = (Get-FileHash -Algorithm SHA256 .\mh3g-save-convert-windows-x64.zip).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "ZIP SHA-256 mismatch: expected $expected, got $actual" }
  Expand-Archive .\mh3g-save-convert-windows-x64.zip -DestinationPath .\mh3g-converter
  Set-Location .\mh3g-converter\mh3g-save-convert-windows-x64

WinUI 发布包中的 Run-Converter.ps1 会先用 tools\mh3g-save-convert.exe.sha256 校验 tools\mh3g-save-convert.exe，再移除这个已固定哈希文件上的 Mark-of-the-Web，然后透传全部参数和退出码。先确认 CLI sidecar 能启动：

  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 --help

输入类型
--------

1. 核心角色：传入一个明确的 user1、user2 或 user3 文件。
   不要传整个 title 目录。目标文件名必须与源槽位名一致。
2. 共享 system：传入一个明确的 system 文件。
3. 可选 ExtData：传入准确的 ...\extdata\00000000\00000481\user 目录。
   该目录必须直接包含 card1/card2/card3/cardbox/quest1/quest2/quest3/quest4。
   不要传 00000481 父目录，也不能只提供部分文件。
4. 可选实验性 CEC：传入准确的 ...\CEC\00048100 目录，其中必须有 InBox___。
   CEC 不是 ExtData，也不是已有公会名片的持久存储。

核心槽位：先只读，再以 Dry Run 哈希受保护地写入，最后可回滚
-------------------------------------------------------------

  $Source = "D:\MH3G-3DS\user2"
  $CemuDir = "D:\Cemu\mlc01\usr\save\00050000\10104D00\user\80000001"
  $Target = Join-Path $CemuDir "user2"

  # 只读检查
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect "$Source"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-progress "$Source" --target "$Target"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-events "$Source" --target "$Target"

  # 必须先 Dry Run。两个哈希必须来自紧接着的这一次输出。
  $CoreDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --dry-run | ConvertFrom-Json
  $CoreSourceHash = $CoreDryRun.hashes.source
  $CoreTargetHash = $CoreDryRun.hashes.target_before
  if ([string]::IsNullOrWhiteSpace($CoreSourceHash) -or [string]::IsNullOrWhiteSpace($CoreTargetHash)) { throw "Core Dry Run did not provide both guarded-write hashes" }

  # 完全退出所有模拟器后才写入
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --expected-source-sha256 "$CoreSourceHash" --expected-target-sha256 "$CoreTargetHash" --write

  # 游戏内验证失败时，保持模拟器关闭并按 manifest 回滚
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback --manifest "$CemuDir\.user2.mh3g-install.json"

system：同样以 Dry Run 哈希受保护地写入
--------------------------------------------

  $SystemSource = "D:\MH3G-3DS\system"
  $SystemTarget = "$CemuDir\system"
  $SystemDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$SystemTarget" --dry-run | ConvertFrom-Json
  $SystemSourceHash = $SystemDryRun.hashes.source
  $SystemTargetHash = $SystemDryRun.hashes.target_before
  if ([string]::IsNullOrWhiteSpace($SystemSourceHash) -or [string]::IsNullOrWhiteSpace($SystemTargetHash)) { throw "System Dry Run did not provide both guarded-write hashes" }
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$SystemTarget" --expected-source-sha256 "$SystemSourceHash" --expected-target-sha256 "$SystemTargetHash" --write

ExtData：完整暂存与只读安装预览（Windows 不会写入多文件组件）
--------------------------------------------------------------

  $ExtData = "D:\MH3G-3DS\extdata\00000000\00000481\user"
  $Staging = "D:\MH3G-Cemu-Extras"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --write

`convert-extras` 必须读取完整的八个文件，并始终把**全部八个**转换到新的暂存目录；它不会直接修改 Cemu。如果八个同名输出中的任何一个已存在，它会拒绝写入。正常迁移不要使用 `--reset-guild-cards`；该参数会生成空白 card* 并丢弃公会名片。

可选：可将完整暂存集与已初始化的 Cemu 存档目录做**只读**组件组预览。`guild-cards` 是 `card1`、`card2`、`card3`、`cardbox`；`quests` 是 `quest1` 到 `quest4`。该预览不创建锁、备份、manifest 或目标更新：

  $ExtrasPreview = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 install-extras --staging-dir "$Staging" --target-dir "$CemuDir" --groups guild-cards,quests --dry-run | ConvertFrom-Json
  $ExtrasPreview | Format-List operation,status,groups,staging_set_sha256,target_set_sha256_before

此 Windows 包会在尚未改动任何 ExtData 文件前，明确拒绝 `install-extras --write` 和 `rollback-extras`。不要手工复制单个 `card*` 或 `quest*`；请在具备完整事务后端的受支持平台（例如当前 macOS 包）完成受保护的安装/回滚。核心 `user#`、共享 `system` 和实验性 CEC 的支持范围不受此限制影响。

CEC：只读检查与实验性写入
-------------------------

  $CecSource = "D:\MH3G-3DS\CEC\00048100"
  $CecTarget = "$CemuDir\cec"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-cec --source-dir "$CecSource" --source-slot "$Source" --target "$CecTarget"

`convert-cec` 是独立的实验功能，默认不启用。它只导入 `InBox___` 中收到的非空消息，故意忽略 `OutBox__`，且不会替代已有名片迁移所需的 `user# + card1 + card2 + card3 + cardbox`。CEC Dry Run 的两个哈希必须紧接着绑定到写入：

  $CecDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-cec --source-dir "$CecSource" --target "$CecTarget" --dry-run | ConvertFrom-Json
  $CecSourceRecordSetHash = $CecDryRun.source_record_set_sha256
  $CecTargetHash = $CecDryRun.target_sha256_before
  if ([string]::IsNullOrWhiteSpace($CecSourceRecordSetHash) -or [string]::IsNullOrWhiteSpace($CecTargetHash)) { throw "CEC Dry Run did not provide both guarded-write hashes" }
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-cec --source-dir "$CecSource" --target "$CecTarget" --slot 0 --expected-source-record-set-sha256 "$CecSourceRecordSetHash" --expected-target-sha256 "$CecTargetHash" --write --experimental

CEC 的成功写入会生成 `$CemuDir\.cec.mh3g-install.json`；验证失败时：

  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback-cec --manifest "$CemuDir\.cec.mh3g-install.json"

事务与权限错误
--------------

- 核心槽位/system 写入会在目标旁生成 `.<name>.mh3g-install.json`；旧目标存在时还会生成按 SHA-256 命名的 backup。
- Windows 的 `convert-extras` 只会生成完整的暂存文件；`install-extras --dry-run` 也完全只读。`convert-cec` 成功写入时会生成 manifest 绑定的恢复事务；保留所有 manifest，直到完成 Cemu 游戏内验证或执行回滚。
- `ConvertFrom-Json` 或显式空值检查会在缺少所需 Dry Run 哈希或 manifest 时停止流程；不要跳过该保护步骤。
- 启动器不能绕过 AppLocker、Smart App Control、杀毒软件或组织策略。
- 如果仍出现 Windows error 5 / Access is denied，请保留 CLI 输出中包含 operation 和 path 的完整错误行，并把 EXE SHA-256 提供给管理员或测试负责人。

====================== English ======================

Scope
-----

- Japanese MH3G 3DS profile 0x2B to Japanese MH3G HD Cemu only.
- Local, one-way conversion. The 3DS source is read-only and nothing is uploaded.
- ZIP, 7z, and RAR are not direct inputs. If using the ZIP distribution, fully extract it to a normal local folder; do not run it from a QQ/browser archive preview. The installer and single-file portable UI run directly from a normal local folder.
- Fully stop Cemu, Azahar, and Nemessix before any supported --write, rollback, or rollback-cec operation.

Windows x64 distribution formats
---------------------------------

Every run of `scripts\package-mh3g-save-converter-windows.ps1` creates the following three formats from the same WinUI release folder and the same Rust CLI sidecar. Each has a matching `.sha256` file:

1. `mh3g-save-convert-windows-x64.zip`: a conventional portable folder. Fully extract it, then run `MH3GSaveConverter.exe`; the folder retains `tools\mh3g-save-convert.exe` and `Run-Converter.ps1`.
2. `MH3GSaveConverter-Setup-x64.exe`: a per-user installer. It installs under the current user's Programs directory and can create Start menu/an optional desktop shortcut without administrator rights.
3. `MH3GSaveConverter-Portable-x64.exe`: a single-file portable UI. Run it directly; no installation or separately placed sidecar is required. First launch extracts the .NET/WinUI runtime and bundled Rust sidecar into a per-user temporary runtime cache. It is a single-file distribution, not a zero-extraction app.

All three are Windows x64 only. Run them from a normal local directory, never a QQ/browser archive preview, and verify the matching `.sha256` after download. The single-file portable UI does not carry `Run-Converter.ps1`; use the ZIP or installed complete folder when explicit CLI access is needed.

Verify and start
----------------

  $expected = ((Get-Content .\mh3g-save-convert-windows-x64.zip.sha256 -Raw).Trim() -split '\s+')[0]
  $actual = (Get-FileHash -Algorithm SHA256 .\mh3g-save-convert-windows-x64.zip).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "ZIP SHA-256 mismatch: expected $expected, got $actual" }
  Expand-Archive .\mh3g-save-convert-windows-x64.zip -DestinationPath .\mh3g-converter
  Set-Location .\mh3g-converter\mh3g-save-convert-windows-x64
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 --help

Input shapes
------------

- Core: one explicit user1/user2/user3 file; source and output basenames must match.
- System: one explicit system file.
- ExtData: exact ...\extdata\00000000\00000481\user directory containing all eight card*/quest* files directly.
- Experimental CEC: exact ...\CEC\00048100 directory containing InBox___.

Core: read only, then a Dry Run hash-guarded write, then rollback
-----------------------------------------------------------------

  $Source = "D:\MH3G-3DS\user2"
  $CemuDir = "D:\Cemu\mlc01\usr\save\00050000\10104D00\user\80000001"
  $Target = Join-Path $CemuDir "user2"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect "$Source"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-progress "$Source" --target "$Target"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-events "$Source" --target "$Target"
  $CoreDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --dry-run | ConvertFrom-Json
  $CoreSourceHash = $CoreDryRun.hashes.source
  $CoreTargetHash = $CoreDryRun.hashes.target_before
  if ([string]::IsNullOrWhiteSpace($CoreSourceHash) -or [string]::IsNullOrWhiteSpace($CoreTargetHash)) { throw "Core Dry Run did not provide both guarded-write hashes" }

  # Run only after every emulator is fully stopped. Both values come from this immediately preceding Dry Run.
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --expected-source-sha256 "$CoreSourceHash" --expected-target-sha256 "$CoreTargetHash" --write

  # Keep every emulator stopped if validation fails.
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback --manifest "$CemuDir\.user2.mh3g-install.json"

System: the same Dry Run hash guard
-----------------------------------

  $SystemSource = "D:\MH3G-3DS\system"
  $SystemTarget = "$CemuDir\system"
  $SystemDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$SystemTarget" --dry-run | ConvertFrom-Json
  $SystemSourceHash = $SystemDryRun.hashes.source
  $SystemTargetHash = $SystemDryRun.hashes.target_before
  if ([string]::IsNullOrWhiteSpace($SystemSourceHash) -or [string]::IsNullOrWhiteSpace($SystemTargetHash)) { throw "System Dry Run did not provide both guarded-write hashes" }
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$SystemTarget" --expected-source-sha256 "$SystemSourceHash" --expected-target-sha256 "$SystemTargetHash" --write

ExtData: complete staging and read-only install preview (Windows does not write multi-file groups)
-----------------------------------------------------------------------------------------------

  $ExtData = "D:\MH3G-3DS\extdata\00000000\00000481\user"
  $Staging = "D:\MH3G-Cemu-Extras"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --write

`convert-extras` requires all eight files and always converts **all eight** to
a fresh staging directory; it does not modify Cemu. It refuses when any
same-named staged output already exists. `--reset-guild-cards` creates empty
guild cards and discards card data, so it is not for a normal migration.

Optional: preview one or both complete component groups against an initialized
Cemu save directory in **read-only** mode. `guild-cards` is `card1`, `card2`,
`card3`, and `cardbox`; `quests` is `quest1` through `quest4`. This preview
creates no lock, backup, manifest, or target update:

  $ExtrasPreview = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 install-extras --staging-dir "$Staging" --target-dir "$CemuDir" --groups guild-cards,quests --dry-run | ConvertFrom-Json
  $ExtrasPreview | Format-List operation,status,groups,staging_set_sha256,target_set_sha256_before

This Windows package deliberately refuses `install-extras --write` and
`rollback-extras` before changing any ExtData file. Do not manually copy a
single `card*` or `quest*` file; complete the guarded install/rollback on a
supported platform with the full transaction backend (for example, the current
macOS package). Core `user#`, shared `system`, and experimental CEC support are
not limited by this ExtData boundary.

CEC: read-only inspection and experimental write
------------------------------------------------

  $CecSource = "D:\MH3G-3DS\CEC\00048100"
  $CecTarget = "$CemuDir\cec"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-cec --source-dir "$CecSource" --source-slot "$Source" --target "$CecTarget"

`convert-cec` is separate and experimental, and is off unless explicitly
requested. It imports only non-empty received records from `InBox___` and
intentionally ignores `OutBox__`. Durable guild cards and offline-hall
partners instead use matching `user# + card1 + card2 + card3 + cardbox`.
Bind both values reported by the immediately preceding CEC Dry Run to a write:

  $CecDryRun = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-cec --source-dir "$CecSource" --target "$CecTarget" --dry-run | ConvertFrom-Json
  $CecSourceRecordSetHash = $CecDryRun.source_record_set_sha256
  $CecTargetHash = $CecDryRun.target_sha256_before
  if ([string]::IsNullOrWhiteSpace($CecSourceRecordSetHash) -or [string]::IsNullOrWhiteSpace($CecTargetHash)) { throw "CEC Dry Run did not provide both guarded-write hashes" }
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-cec --source-dir "$CecSource" --target "$CecTarget" --slot 0 --expected-source-record-set-sha256 "$CecSourceRecordSetHash" --expected-target-sha256 "$CecTargetHash" --write --experimental

  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback-cec --manifest "$CemuDir\.cec.mh3g-install.json"

Transaction notes
-----------------

- Core/system writes create `.<name>.mh3g-install.json`; an existing target also receives a SHA-256-named backup.
- On Windows, `convert-extras` creates complete staging files only and `install-extras --dry-run` remains read-only. A successful `convert-cec` write creates a manifest-bound recovery transaction; keep every manifest until Cemu validation passes or rollback finishes.
- `ConvertFrom-Json` and the explicit empty-value checks stop these examples when a required Dry Run hash or manifest is absent. Do not skip that guard.

In the WinUI release package, Run-Converter.ps1 verifies tools\mh3g-save-convert.exe against tools\mh3g-save-convert.exe.sha256 before removing Mark-of-the-Web from that hash-pinned sidecar. It cannot bypass system or organization application-control policy. Keep the complete operation and path if Windows reports error 5 (Access is denied). See the repository README for every command and option.
