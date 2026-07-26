MH3G 日版 3DS -> Wii U/Cemu 存档转换器（Windows x64）
MH3G Japanese 3DS -> Wii U/Cemu Save Converter (Windows x64)

==================== 简体中文 ====================

适用范围
--------

- 仅支持日版 MH3G 3DS 存档（0x2B profile）转换到日版 MH3G HD Cemu。
- 这是本地、单向转换；不会上传或修改 3DS 源文件，不能从 Cemu 转回 3DS。
- 不支持直接读取 ZIP、7z、RAR，也不会自动扫描整个存档目录。
- 必须先把下载 artifact 和内部 ZIP 完整解压到普通本地目录。不要在 QQ/浏览器压缩包预览中运行 EXE。
- 执行任何 --write、rollback 或 rollback-cec 前，必须完全退出 Cemu、Azahar 和 Nemessix。

先校验下载的 ZIP（在 ZIP 和 .sha256 所在目录执行）
--------------------------------------------------

  $expected = ((Get-Content .\mh3g-save-convert-windows-x64.zip.sha256 -Raw).Trim() -split '\s+')[0]
  $actual = (Get-FileHash -Algorithm SHA256 .\mh3g-save-convert-windows-x64.zip).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "ZIP SHA-256 mismatch: expected $expected, got $actual" }
  Expand-Archive .\mh3g-save-convert-windows-x64.zip -DestinationPath .\mh3g-converter
  Set-Location .\mh3g-converter\mh3g-save-convert-windows-x64

Run-Converter.ps1 会先用 mh3g-save-convert.exe.sha256 校验 EXE，再移除这个已固定哈希文件上的 Mark-of-the-Web，然后透传全部参数和退出码。先确认程序能启动：

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

推荐的核心槽位流程
------------------

  $Source = "D:\MH3G-3DS\user2"
  $CemuDir = "D:\Cemu\mlc01\usr\save\00050000\10104D00\user\80000001"
  $Target = Join-Path $CemuDir "user2"

  # 只读检查
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect "$Source"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-progress "$Source" --target "$Target"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-events "$Source" --target "$Target"

  # 必须先 dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --dry-run

  # 完全退出所有模拟器后才写入
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --write

  # 游戏内验证失败时，保持模拟器关闭并按 manifest 回滚
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback --manifest "$CemuDir\.user2.mh3g-install.json"

system 示例
-----------

  $SystemSource = "D:\MH3G-3DS\system"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$CemuDir\system" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "$SystemSource" --output "$CemuDir\system" --write

ExtData 示例
------------

  $ExtData = "D:\MH3G-3DS\extdata\00000000\00000481\user"
  $Staging = "D:\MH3G-Cemu-Extras"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "$ExtData" --output-dir "$Staging" --write

convert-extras 只会写入新的暂存目录；如果八个同名输出中的任何一个已存在，它会拒绝写入。它不会自动安装到 Cemu，也不会为手动安装的 card*/quest* 创建备份。正常迁移不要使用 --reset-guild-cards；该参数会生成空白 card* 并丢弃公会名片。

CEC 只读示例
------------

  $CecSource = "D:\MH3G-3DS\CEC\00048100"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect-cec --source-dir "$CecSource" --source-slot "$Source" --target "$CemuDir\cec"

convert-cec --write 必须同时传入 --experimental。它只考虑 InBox___ 中收到的非空消息，故意忽略 OutBox__，并且不会替代已有名片迁移所需的 user# + card1 + card2 + card3 + cardbox。

事务与权限错误
--------------

- 核心槽位/system 写入会在目标旁生成 .<name>.mh3g-install.json；旧目标存在时还会生成按 SHA-256 命名的 backup。
- 保留 manifest，直到完成 Cemu 游戏内验证或执行回滚。
- 启动器不能绕过 AppLocker、Smart App Control、杀毒软件或组织策略。
- 如果仍出现 Windows error 5 / Access is denied，请保留 CLI 输出中包含 operation 和 path 的完整错误行，并把 EXE SHA-256 提供给管理员或测试负责人。

====================== English ======================

Scope
-----

- Japanese MH3G 3DS profile 0x2B to Japanese MH3G HD Cemu only.
- Local, one-way conversion. The 3DS source is read-only and nothing is uploaded.
- ZIP, 7z, and RAR are not direct inputs. Extract the complete package to a normal local folder; do not run it from a QQ/browser archive preview.
- Fully stop Cemu, Azahar, and Nemessix before any --write, rollback, or rollback-cec.

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

Core dry-run, write, and rollback
---------------------------------

  $Source = "D:\MH3G-3DS\user2"
  $CemuDir = "D:\Cemu\mlc01\usr\save\00050000\10104D00\user\80000001"
  $Target = Join-Path $CemuDir "user2"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 inspect "$Source"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert "$Source" --output "$Target" --write
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 rollback --manifest "$CemuDir\.user2.mh3g-install.json"

System and ExtData
------------------

  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-system "D:\MH3G-3DS\system" --output "$CemuDir\system" --dry-run
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Run-Converter.ps1 convert-extras --source-dir "D:\MH3G-3DS\extdata\00000000\00000481\user" --output-dir "D:\MH3G-Cemu-Extras" --dry-run

Run convert-extras --write only with a fresh staging directory. It does not install staged files into Cemu or back up a manual installation. --reset-guild-cards intentionally discards guild cards and is not a normal migration option.

Run-Converter.ps1 verifies the packaged EXE hash before removing Mark-of-the-Web from that hash-pinned file. It cannot bypass system or organization application-control policy. Keep the complete operation and path if Windows reports error 5 (Access is denied). See the repository README for every command and option.
