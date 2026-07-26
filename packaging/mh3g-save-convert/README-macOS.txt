MH3G 日版 3DS -> Wii U/Cemu 存档转换器（macOS arm64）
MH3G Japanese 3DS -> Wii U/Cemu Save Converter (macOS arm64)

==================== 简体中文 ====================

适用范围
--------

- 仅支持 Apple Silicon（arm64）macOS 上的日版 MH3G 0x2B profile。
- 只支持 3DS -> 日版 MH3G HD Cemu；不会上传或修改 3DS 源文件。
- 不能直接读取 ZIP、7z、RAR，也不会从整个存档目录自动寻找文件。
- 必须先完整解压 ZIP，再从 Terminal 进入解压后的目录执行。
- 执行任何 --write、rollback 或 rollback-cec 前，完全退出 Cemu、Azahar 和 Nemessix。

校验与启动
----------

先在下载 ZIP 和 sidecar 的目录校验外层压缩包：

  expected="$(awk '{print $1}' mh3g-save-convert-macos-arm64.zip.sha256)"
  actual="$(shasum -a 256 mh3g-save-convert-macos-arm64.zip | awk '{print $1}')"
  test "$actual" = "$expected" || { echo "ZIP SHA-256 mismatch" >&2; exit 1; }

完整解压并进入目录后，校验二进制：

  shasum -a 256 -c mh3g-save-convert.sha256
  ./mh3g-save-convert --help

macOS ZIP 会保留可执行权限。如果第三方传输工具丢失权限，只在 SHA-256 校验成功后执行：

  chmod +x ./mh3g-save-convert

如果下载工具增加了 quarantine，并且 Terminal 阻止启动，只在 SHA-256 校验成功后执行：

  xattr -d com.apple.quarantine ./mh3g-save-convert 2>/dev/null || true

输入类型
--------

1. 核心角色：一个明确的 user1、user2 或 user3 文件；源和目标文件名必须相同。
2. 共享 system：一个明确的 system 文件。
3. 可选 ExtData：准确的 .../extdata/00000000/00000481/user 目录，直接包含全部八个 card*/quest* 文件。
4. 可选实验性 CEC：准确的 .../CEC/00048100 目录，其中包含 InBox___。

核心槽位：先只读，再写入，最后可回滚
--------------------------------------

  SOURCE="$HOME/Desktop/MH3G-3DS/user2"
  CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
  TARGET="$CEMU_DIR/user2"

  ./mh3g-save-convert inspect "$SOURCE"
  ./mh3g-save-convert inspect-progress "$SOURCE" --target "$TARGET"
  ./mh3g-save-convert inspect-events "$SOURCE" --target "$TARGET"
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --dry-run

  # 完全退出所有模拟器后才执行：
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --write

  # 游戏内验证失败时，保持模拟器关闭：
  ./mh3g-save-convert rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"

system 和 ExtData
-----------------

  SYSTEM_SOURCE="$HOME/Desktop/MH3G-3DS/system"
  EXTRAS_SOURCE="$HOME/Desktop/MH3G-3DS/extdata/00000000/00000481/user"
  EXTRAS_OUTPUT="$HOME/Desktop/MH3G-Cemu-Extras"

  ./mh3g-save-convert convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --dry-run
  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write

convert-extras 要求全部八个文件，并且只写入新的暂存目录。它不会自动安装到 Cemu，也不会备份手动安装的 card*/quest*。--reset-guild-cards 会生成空白名片文件，正常迁移不要使用。

CEC 只读检查
------------

  CEC_SOURCE="$HOME/Desktop/MH3G-3DS/CEC/00048100"
  ./mh3g-save-convert inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEMU_DIR/cec"

convert-cec --write 是独立实验功能，必须同时传入 --experimental。它只导入 InBox___ 中收到的非空记录，故意忽略 OutBox__。已有公会名片和离线伙伴依赖的是编号匹配的 user# + card1 + card2 + card3 + cardbox，而不是 CEC。

事务说明
--------

- core/system 写入会生成 .<name>.mh3g-install.json；旧目标存在时还会创建按 SHA-256 命名的 backup。
- 保留 manifest，直到 Cemu 游戏内验证完成或已经回滚。
- convert-extras 没有覆盖、backup 或 rollback 安装器；只能写入新暂存目录。
- 如果命令失败，请保留完整错误、操作名和路径。

====================== English ======================

Scope
-----

- Apple Silicon (arm64) macOS and Japanese MH3G profile 0x2B only.
- One-way 3DS to Japanese MH3G HD Cemu conversion; the source is read-only and nothing is uploaded.
- ZIP, 7z, and RAR are not direct inputs. Fully extract the archive first.
- Fully stop Cemu, Azahar, and Nemessix before any --write, rollback, or rollback-cec.

Verify and start
----------------

  expected="$(awk '{print $1}' mh3g-save-convert-macos-arm64.zip.sha256)"
  actual="$(shasum -a 256 mh3g-save-convert-macos-arm64.zip | awk '{print $1}')"
  test "$actual" = "$expected" || { echo "ZIP SHA-256 mismatch" >&2; exit 1; }

After full extraction:

  shasum -a 256 -c mh3g-save-convert.sha256
  ./mh3g-save-convert --help

The ZIP preserves executable mode. If a third-party transfer tool removes it, run chmod +x only after the SHA-256 check. If quarantine blocks a verified binary, remove that attribute only after the same hash check.

Input shapes
------------

- Core: one explicit user1/user2/user3 file; source and target basenames must match.
- System: one explicit system file.
- ExtData: exact .../extdata/00000000/00000481/user directory containing all eight card*/quest* files directly.
- Experimental CEC: exact .../CEC/00048100 directory containing InBox___.

Core flow
---------

  SOURCE="$HOME/Desktop/MH3G-3DS/user2"
  CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
  TARGET="$CEMU_DIR/user2"
  ./mh3g-save-convert inspect "$SOURCE"
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --dry-run
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --write
  ./mh3g-save-convert rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"

System and ExtData
------------------

  ./mh3g-save-convert convert-system "$HOME/Desktop/MH3G-3DS/system" --output "$CEMU_DIR/system" --dry-run
  ./mh3g-save-convert convert-extras --source-dir "$HOME/Desktop/MH3G-3DS/extdata/00000000/00000481/user" --output-dir "$HOME/Desktop/MH3G-Cemu-Extras" --dry-run

Use a fresh staging directory for convert-extras --write. It does not install into Cemu or back up manual installation. --reset-guild-cards intentionally discards guild cards and is not a normal migration option. See the repository README for every command and option.
