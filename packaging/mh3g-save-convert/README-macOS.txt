MH3G 日版 3DS -> Wii U/Cemu 存档转换器（macOS arm64）
MH3G Japanese 3DS -> Wii U/Cemu Save Converter (macOS arm64)

==================== 简体中文 ====================

适用范围
--------

- 仅支持 Apple Silicon（arm64）macOS 上的日版 MH3G 0x2B profile。
- 只支持 3DS -> 日版 MH3G HD Cemu；不会上传或修改 3DS 源文件。
- 不能直接读取 ZIP、7z、RAR，也不会从整个存档目录自动寻找文件。
- 必须先完整解压 ZIP，再从 Terminal 进入解压后的目录执行。
- 执行任何 --write、rollback、rollback-extras 或 rollback-cec 前，完全退出 Cemu、Azahar 和 Nemessix。

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

核心槽位：先只读，再以 Dry Run 哈希受保护地写入，最后可回滚
-------------------------------------------------------------

  SOURCE="$HOME/Desktop/MH3G-3DS/user2"
  CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
  TARGET="$CEMU_DIR/user2"

  ./mh3g-save-convert inspect "$SOURCE"
  ./mh3g-save-convert inspect-progress "$SOURCE" --target "$TARGET"
  ./mh3g-save-convert inspect-events "$SOURCE" --target "$TARGET"
  CORE_DRY_RUN_JSON=$(./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --dry-run)
  CORE_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$CORE_DRY_RUN_JSON")
  CORE_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$CORE_DRY_RUN_JSON")

  # 完全退出所有模拟器后才执行；两项哈希必须来自紧接着的同一次 Dry Run：
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" \
    --expected-source-sha256 "$CORE_SOURCE_SHA256" \
    --expected-target-sha256 "$CORE_TARGET_SHA256" \
    --write

  # 游戏内验证失败时，保持模拟器关闭：
  ./mh3g-save-convert rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"

system：同样以 Dry Run 哈希受保护地写入
--------------------------------------------

  SYSTEM_SOURCE="$HOME/Desktop/MH3G-3DS/system"
  SYSTEM_TARGET="$CEMU_DIR/system"
  SYSTEM_DRY_RUN_JSON=$(./mh3g-save-convert convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" --dry-run)
  SYSTEM_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$SYSTEM_DRY_RUN_JSON")
  SYSTEM_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$SYSTEM_DRY_RUN_JSON")

  ./mh3g-save-convert convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" \
    --expected-source-sha256 "$SYSTEM_SOURCE_SHA256" \
    --expected-target-sha256 "$SYSTEM_TARGET_SHA256" \
    --write

ExtData：暂存、事务安装与回滚
------------------------------

  EXTRAS_SOURCE="$HOME/Desktop/MH3G-3DS/extdata/00000000/00000481/user"
  EXTRAS_OUTPUT="$HOME/Desktop/MH3G-Cemu-Extras"

  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write

`convert-extras` 必须读取全部八个文件，并且只能写入新的暂存目录；它不会直接修改 Cemu。
`--reset-guild-cards` 会生成空白名片文件并丢弃公会名片，正常迁移不要使用。

不要手工复制单个 `card*` 或 `quest*`。使用 `install-extras` 对完整组件组做事务安装。目标必须是已初始化的 MH3G Cemu 存档目录，且已含被选择的同名组件：

  EXTRAS_INSTALL_DRY_RUN_JSON=$(./mh3g-save-convert install-extras \
    --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
    --groups guild-cards,quests --dry-run)
  EXTRAS_STAGING_SHA256=$(jq -er '.staging_set_sha256' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")
  EXTRAS_TARGET_SHA256=$(jq -er '.target_set_sha256_before' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")

  EXTRAS_WRITE_JSON=$(./mh3g-save-convert install-extras \
    --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
    --groups guild-cards,quests \
    --expected-staging-set-sha256 "$EXTRAS_STAGING_SHA256" \
    --expected-target-set-sha256 "$EXTRAS_TARGET_SHA256" \
    --write)
  EXTRAS_MANIFEST=$(jq -er '.manifest' <<<"$EXTRAS_WRITE_JSON")

`guild-cards` 是 `card1`、`card2`、`card3`、`cardbox`；`quests` 是 `quest1` 到 `quest4`。只能选择完整组。安装会创建绑定 manifest 的备份事务；验证失败时保持模拟器关闭并回滚：

  ./mh3g-save-convert rollback-extras --manifest "$EXTRAS_MANIFEST"

CEC：只读检查与实验性写入
-------------------------

  CEC_SOURCE="$HOME/Desktop/MH3G-3DS/CEC/00048100"
  CEC_TARGET="$CEMU_DIR/cec"
  ./mh3g-save-convert inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEMU_DIR/cec"

`convert-cec` 是独立的实验功能，默认不启用。它只导入 `InBox___` 中收到的非空记录，并故意忽略 `OutBox__`。已有公会名片和离线伙伴依赖编号匹配的 `user# + card1 + card2 + card3 + cardbox`，不是 CEC。CEC Dry Run 的两个哈希必须紧接着绑定到写入：

  CEC_DRY_RUN_JSON=$(./mh3g-save-convert convert-cec \
    --source-dir "$CEC_SOURCE" --target "$CEC_TARGET" --dry-run)
  CEC_SOURCE_RECORD_SET_SHA256=$(jq -er '.source_record_set_sha256' <<<"$CEC_DRY_RUN_JSON")
  CEC_TARGET_SHA256=$(jq -er '.target_sha256_before' <<<"$CEC_DRY_RUN_JSON")

  ./mh3g-save-convert convert-cec \
    --source-dir "$CEC_SOURCE" --target "$CEC_TARGET" --slot 0 \
    --expected-source-record-set-sha256 "$CEC_SOURCE_RECORD_SET_SHA256" \
    --expected-target-sha256 "$CEC_TARGET_SHA256" \
    --write --experimental

CEC 的成功写入会生成 `$CEMU_DIR/.cec.mh3g-install.json`；验证失败时：

  ./mh3g-save-convert rollback-cec --manifest "$CEMU_DIR/.cec.mh3g-install.json"

事务说明
--------

- core/system 写入会生成 `.<name>.mh3g-install.json`；旧目标存在时还会创建按 SHA-256 命名的 backup。
- `install-extras` 和 `convert-cec` 也会生成 manifest 绑定的恢复事务；保留所有 manifest，直到 Cemu 游戏内验证完成或已经回滚。
- `jq -e` 在缺少所需 Dry Run 哈希或 manifest 时会停止脚本；不要跳过该保护步骤。
- 如果命令失败，请保留完整错误、操作名和路径。

====================== English ======================

Scope
-----

- Apple Silicon (arm64) macOS and Japanese MH3G profile 0x2B only.
- One-way 3DS to Japanese MH3G HD Cemu conversion; the source is read-only and nothing is uploaded.
- ZIP, 7z, and RAR are not direct inputs. Fully extract the archive first.
- Fully stop Cemu, Azahar, and Nemessix before any --write, rollback, rollback-extras, or rollback-cec.

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

Core: read only, then a Dry Run hash-guarded write, then rollback
-----------------------------------------------------------------

  SOURCE="$HOME/Desktop/MH3G-3DS/user2"
  CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
  TARGET="$CEMU_DIR/user2"
  ./mh3g-save-convert inspect "$SOURCE"
  ./mh3g-save-convert inspect-progress "$SOURCE" --target "$TARGET"
  ./mh3g-save-convert inspect-events "$SOURCE" --target "$TARGET"
  CORE_DRY_RUN_JSON=$(./mh3g-save-convert convert "$SOURCE" --output "$TARGET" --dry-run)
  CORE_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$CORE_DRY_RUN_JSON")
  CORE_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$CORE_DRY_RUN_JSON")

  # Run only after every emulator is fully stopped. Both values come from this immediately preceding Dry Run.
  ./mh3g-save-convert convert "$SOURCE" --output "$TARGET" \
    --expected-source-sha256 "$CORE_SOURCE_SHA256" \
    --expected-target-sha256 "$CORE_TARGET_SHA256" \
    --write

  # Keep every emulator stopped if validation fails:
  ./mh3g-save-convert rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"

System: the same Dry Run hash guard
-----------------------------------

  SYSTEM_SOURCE="$HOME/Desktop/MH3G-3DS/system"
  SYSTEM_TARGET="$CEMU_DIR/system"
  SYSTEM_DRY_RUN_JSON=$(./mh3g-save-convert convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" --dry-run)
  SYSTEM_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$SYSTEM_DRY_RUN_JSON")
  SYSTEM_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$SYSTEM_DRY_RUN_JSON")

  ./mh3g-save-convert convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" \
    --expected-source-sha256 "$SYSTEM_SOURCE_SHA256" \
    --expected-target-sha256 "$SYSTEM_TARGET_SHA256" \
    --write

ExtData: stage, transactionally install, and roll back
------------------------------------------------------

  EXTRAS_SOURCE="$HOME/Desktop/MH3G-3DS/extdata/00000000/00000481/user"
  EXTRAS_OUTPUT="$HOME/Desktop/MH3G-Cemu-Extras"

  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
  ./mh3g-save-convert convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write

`convert-extras` requires all eight files and writes only to a fresh staging
directory; it does not modify Cemu. `--reset-guild-cards` creates empty guild
cards and discards card data, so it is not for a normal migration.

Do not manually copy a single `card*` or `quest*` file. Use `install-extras`
for an all-or-nothing component-group transaction. The target must be an
initialized MH3G Cemu save directory containing the selected named components:

  EXTRAS_INSTALL_DRY_RUN_JSON=$(./mh3g-save-convert install-extras \
    --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
    --groups guild-cards,quests --dry-run)
  EXTRAS_STAGING_SHA256=$(jq -er '.staging_set_sha256' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")
  EXTRAS_TARGET_SHA256=$(jq -er '.target_set_sha256_before' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")

  EXTRAS_WRITE_JSON=$(./mh3g-save-convert install-extras \
    --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
    --groups guild-cards,quests \
    --expected-staging-set-sha256 "$EXTRAS_STAGING_SHA256" \
    --expected-target-set-sha256 "$EXTRAS_TARGET_SHA256" \
    --write)
  EXTRAS_MANIFEST=$(jq -er '.manifest' <<<"$EXTRAS_WRITE_JSON")

`guild-cards` is `card1`, `card2`, `card3`, and `cardbox`; `quests` is
`quest1` through `quest4`. Only whole groups are supported. A write retains a
manifest-bound recovery transaction; with emulators stopped, roll back after a
failed validation:

  ./mh3g-save-convert rollback-extras --manifest "$EXTRAS_MANIFEST"

CEC: read-only inspection and experimental write
------------------------------------------------

  CEC_SOURCE="$HOME/Desktop/MH3G-3DS/CEC/00048100"
  CEC_TARGET="$CEMU_DIR/cec"
  ./mh3g-save-convert inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEC_TARGET"

`convert-cec` is separate and experimental, and is off unless explicitly
requested. It imports only non-empty received records from `InBox___` and
intentionally ignores `OutBox__`. Durable guild cards and offline-hall
partners instead use matching `user# + card1 + card2 + card3 + cardbox`.
Bind both values reported by the immediately preceding CEC Dry Run to a write:

  CEC_DRY_RUN_JSON=$(./mh3g-save-convert convert-cec \
    --source-dir "$CEC_SOURCE" --target "$CEC_TARGET" --dry-run)
  CEC_SOURCE_RECORD_SET_SHA256=$(jq -er '.source_record_set_sha256' <<<"$CEC_DRY_RUN_JSON")
  CEC_TARGET_SHA256=$(jq -er '.target_sha256_before' <<<"$CEC_DRY_RUN_JSON")

  ./mh3g-save-convert convert-cec \
    --source-dir "$CEC_SOURCE" --target "$CEC_TARGET" --slot 0 \
    --expected-source-record-set-sha256 "$CEC_SOURCE_RECORD_SET_SHA256" \
    --expected-target-sha256 "$CEC_TARGET_SHA256" \
    --write --experimental

  ./mh3g-save-convert rollback-cec --manifest "$CEMU_DIR/.cec.mh3g-install.json"

Transaction notes
-----------------

- Core/system writes create `.<name>.mh3g-install.json`; an existing target also receives a SHA-256-named backup.
- `install-extras` and `convert-cec` create manifest-bound recovery transactions. Keep every manifest until Cemu validation passes or rollback finishes.
- `jq -e` stops these examples when a required Dry Run hash or manifest is absent. Do not skip that guard.
- Preserve the complete error including operation and path if a command fails. See the repository README for every command and option.
