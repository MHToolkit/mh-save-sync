# MH Save Sync

[English](README.md) | [简体中文](README.zh-CN.md)

Cross-platform, multi-emulator save synchronization for macOS and Android with
end-to-end encrypted snapshots and a self-hosted service.

This repository is in **research/alpha**. It must not be treated as a stable
backup product until the data-integrity gates in `docs/ROADMAP.md` pass.

## Safety invariants

- Local emulator saves remain in their original format and location.
- File watchers mark saves dirty; they never upload directly.
- Remote data is never written into a running emulator's save directory.
- Concurrent histories become conflict branches, never silent last-write-wins.
- A restore snapshots the current state before replacing anything.
- The service never receives recovery secrets or plaintext save contents.

## Japanese MH3G 3DS -> Cemu conversion (offline)

`mh3g-save-convert` migrates **one Japanese MH3G 3DS slot** to the matching
Japanese MH3G HD Cemu slot. It is local-only and one-way: it never uploads a
save, never modifies the source file, does not support another region, and does
not convert Cemu back to 3DS. It preserves bytes outside documented conversion
ranges, but preserved bytes are not proof that every in-game field has the same
meaning on both platforms.

### Native macOS workbench (development)

`apps/mh3g-save-converter-macos` is a separate foreground SwiftUI app, not the
existing MH Save Sync menu-bar client. It uses the bundled
`mh3g-save-convert` executable through an argv array and its JSON reports; it
does not implement byte conversion, backup, manifest, process checks, or
rollback itself. The window appears in the Dock and Cmd-Tab, starts in the
four-stage workbench, follows the system language by default, and can switch
between Simplified Chinese and English in Settings.

The app accepts only explicitly selected `user1`, `user2`, `user3`, `system`,
ExtData, and CEC paths. It does not discover an MLC root, scan a directory, or
accept archive files. A core write remains disabled until its current Dry Run
fingerprint matches the selected source SHA-256, target SHA-256, and component
scope. CEC is a separate collapsed experimental page and is never needed for
the normal guild-card/offline-partner group.

On an arm64 macOS development host, build and exercise only synthetic fixtures:

```bash
bash scripts/build-mh3g-save-converter-macos-app.sh
bash scripts/mh3g-save-converter-macos-smoke.sh
bash scripts/package-mh3g-save-converter-macos.sh
```

The smoke test creates a temporary zero-content fixture, verifies inspect,
dry-run, transactional write, manifest-bound rollback, and source hash
preservation, then removes the temporary directory. It never launches Cemu or
opens a real MLC.

> **Read this first:** the current CLI does **not** read ZIP, 7z, or RAR
> archives directly, and it does **not** search an arbitrary save directory for
> a plausible file. Fully extract an archive to a normal local directory, then
> give the command the exact file or exact directory shown below. Do not run a
> program from a QQ/browser archive preview. Quote every path that contains a
> space.

Before any `--write`, `rollback`, or `rollback-cec`, fully quit Nemessix,
Azahar, and Cemu and wait for their processes to stop. `inspect`,
`inspect-progress`, `inspect-events`, `inspect-cec`, and every `--dry-run` are
read-only.

### Select the right extracted input

The Japanese MH3G title savedata and its shared data live in three different
places. The ExtData root is `00000481`, but `convert-extras` needs its `user`
child. CEC is a system NAND mailbox, not SD-card ExtData.

| Data group | Give this exact input to the CLI | Do **not** give it | Required? | Purpose / affected files |
| --- | --- | --- | --- | --- |
| Core slot | One explicit `user1`, `user2`, or `user3` file under `title/00040000/00048100/data/00000001/` | The title directory, all slots, ExtData, a ZIP | Yes: choose one slot | Character, story/progress, farm, fleet, local offline-hunter data; writes only the named same-number Cemu `user#` target |
| Shared system | One explicit `system` file in the same title savedata directory | The whole title directory, a ZIP | Optional | Shared system data; writes only the named Cemu `system` target |
| Shared ExtData | The complete `extdata/00000000/00000481/user/` directory containing `card1`, `card2`, `card3`, `cardbox`, `quest1`, `quest2`, `quest3`, `quest4` directly inside | The `00000481` parent, `boss/`, a partial set, a ZIP | Optional | Generates converted `card*` and `quest*` files in a new staging directory |
| StreetPass / Hunter Search CEC | The exact `CEC/00048100/` directory containing `InBox___` | SD-card ExtData, the `InBox___` child alone, a ZIP | Optional and experimental | Reads received raw StreetPass records and can write only Cemu `cec` |

For Nemessix, replace `<ID0>` and `<ID1>` with the two 32-hexadecimal
directory names below `sdmc/Nintendo 3DS/`; zero-filled IDs are only a common
local emulator example:

```text
3DS title savedata
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/title/00040000/00048100/data/00000001/
    user1  user2  user3  system

3DS MH3G shared ExtData
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/extdata/00000000/00000481/
    user/                         <- pass this directory to convert-extras
      card1 card2 card3 cardbox quest1 quest2 quest3 quest4
    boss/ icon metadata            <- not converter inputs

3DS system StreetPass CEC mailbox
  .../nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100/
    InBox___/BoxInfo_____ and InBox___/_*  <- received messages
    OutBox__/...                           <- local outgoing broadcast
```

If an extracted folder contains `user2`, choose that **file** for a core slot
conversion. If it contains `card1` through `quest4`, choose that folder only
for `convert-extras`. If it contains an extra wrapper directory, enter it first;
the expected filenames must be immediate children of the path passed to the
command.

### Before you write: paths, inspection, and dry-run

The examples below run from this repository after Rust is installed. Define a
shell array once; when using a packaged binary, replace it with
`CLI=("/path/to/mh3g-save-convert")`.

```bash
CLI=(cargo run --quiet -p mh3g-save-convert --)

# Replace the two IDs and the Cemu user directory with your own extracted paths.
N3DS_ROOT="$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/<ID0>/<ID1>"
SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/user2"
SYSTEM_SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/system"
EXTRAS_SOURCE="$N3DS_ROOT/extdata/00000000/00000481/user"
CEC_SOURCE="$HOME/Library/Application Support/Nemessix/nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100"

# Example only: choose the existing Cemu account directory that contains user# files.
CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
TARGET="$CEMU_DIR/user2"
CEMU_CEC="$CEMU_DIR/cec"

"${CLI[@]}" --help
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET"
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET"
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

`inspect` takes exactly one `user#` or `system` file and writes nothing.
`inspect-progress` takes a source slot, optional `--target <Cemu-user#>`, and
optional `--quest-id <0..65535>` to restrict output. `inspect-events` takes a
source slot, optional `--target <Cemu-user#>`, and optional `--all` to include
unset event coordinates. The progress decoder maps quest IDs, including the 16
completion words at payload offset `0x6E5C`; the event decoder covers 58 simple
event words at `0x62AE` and the categorized table at `0x668C`.

`convert` accepts one source `user#` file and `--output <same-name-user#>`.
`user2` can only write a target named `user2`; it cannot overwrite `user1` or
an arbitrary renamed file. With no `--write`, conversion remains a dry-run;
pass `--dry-run` explicitly in scripts to make that intention visible. `--write`
and `--dry-run` conflict.

### Complete command reference

All commands below use the `CLI` array above. Replace it with a packaged binary
if you are not building from source.

#### `inspect` — read one file

```text
mh3g-save-convert inspect <SOURCE>
```

`<SOURCE>` is one recognized Japanese 3DS or Cemu `user1`/`user2`/`user3` or
`system` file. It validates the 3DS `0x2B` profile or the Cemu container,
reports profile/size/hash information, and writes nothing. This is also the
readback check for a converted output:

```bash
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect "$SYSTEM_SOURCE"
```

#### `inspect-progress` — read quest completion

```text
mh3g-save-convert inspect-progress [--target <TARGET>] [--quest-id <QUEST_ID>] <SOURCE>
```

`<SOURCE>` is one 3DS `user#`; `--target` is an optional same-slot Cemu
`user#`; `--quest-id` filters to one numeric quest ID. It writes nothing:

```bash
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET" --quest-id 201
```

#### `inspect-events` — read story/event flags

```text
mh3g-save-convert inspect-events [--target <TARGET>] [--all] <SOURCE>
```

`<SOURCE>` and optional `--target` are the same kind of files as above.
`--all` adds unset coordinates; without it the report focuses on active values.
It writes nothing:

```bash
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET" --all
```

#### `convert` — convert one character slot

```text
mh3g-save-convert convert [--dry-run | --write] --output <OUTPUT> <SOURCE>
```

`<SOURCE>` and `<OUTPUT>` must have the same `user#` basename. Read-only use:

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

With all emulators stopped, install atomically:

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --write
```

If a target existed, `--write` creates
`.user2.mh3g-backup-<previous-sha256>` beside it, plus
`.user2.mh3g-install.json`; repeated installs may create
`.user2.mh3g-install-history-<sha256>.json`. Keep the manifest until manual
Cemu validation succeeds.

#### `convert-system` — convert shared system data

```text
mh3g-save-convert convert-system [--dry-run | --write] --output <OUTPUT> <SOURCE>
```

Use explicit `system` files only; it never reads a `user#` or ExtData:

```bash
"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --dry-run
"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --write
```

The same transactional backup/manifest pattern applies, using `.system...`
names. `--write` and `--dry-run` conflict.

#### `convert-extras` — stage shared ExtData

```text
mh3g-save-convert convert-extras [--dry-run | --write] [--reset-guild-cards] \
  --source-dir <EXTDATA-USER-DIR> --output-dir <NEW-STAGING-DIR>
```

`--source-dir` must be the complete `.../extdata/00000000/00000481/user/`
directory; all eight files are required even if you later install only card
files. `--output-dir` is a new staging directory. Existing named component
files are refused, so this command never overwrites a Cemu save:

```bash
EXTRAS_OUTPUT="$HOME/Desktop/mh3g-cemu-extras"
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write
```

`quest1`–`quest4` receive the Cemu container. `card1`–`card3` and `cardbox`
receive the recovered cross-platform field mapping before the wrapper is
written. `--reset-guild-cards` is an explicit destructive recovery switch: it
generates empty native-Cemu `card*` files and discards local/received card data.
Do not use it for normal migration. Back up a Cemu destination before manually
installing generated files.

#### `inspect-cec` — read StreetPass/Hunter Search mailbox

```text
mh3g-save-convert inspect-cec --source-dir <CEC-DIR> [--target <CEMU-CEC>] \
  [--source-slot <USER-SLOT>]
```

`--source-dir` is the CEC `.../CEC/00048100/` directory, not ExtData.
`--target` optionally reads a Cemu `cec` file. `--source-slot` optionally reads
one `user#` only to locate its guild-card anchor. The command writes nothing:

```bash
"${CLI[@]}" inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEMU_CEC"
```

#### `convert-cec` — experimental received-message import

```text
mh3g-save-convert convert-cec [--dry-run | --write --experimental] \
  [--slot <SLOT>] --source-dir <CEC-DIR> --target <CEMU-CEC>
```

CEC is not the main save and not the durable guild-card store. `InBox___/_*`
are raw received messages; `OutBox__/_*` are the local hunter's outgoing
broadcast and are intentionally ignored. `BoxInfo_____` is mailbox metadata.
Only non-empty inbox records are candidates. Existing guild cards and
offline-hall partners use this durable set instead:

```text
matching user# + card1 + card2 + card3 + cardbox
```

An empty CEC inbox is normal even when the durable card list is non-empty.
`convert-cec` is independent and **experimental**: it has file-level evidence,
not a blanket runtime guarantee for every Wii U UI. It writes no `user#`,
`system`, `card*`, or `quest*` file. It uses the first empty Cemu slot by
default; `--slot <SLOT>` chooses the first candidate slot and never overwrites
an existing non-empty record.

```bash
"${CLI[@]}" convert-cec --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --dry-run
"${CLI[@]}" convert-cec --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --slot 0 --write --experimental
```

The source 3DS wrapper and observed 8-byte message prefix are not copied. A
successful CEC write creates `.cec.mh3g-backup-<previous-sha256>` when needed
and `.cec.mh3g-install.json` beside the target.

#### `rollback` and `rollback-cec` — restore only a known transaction

```text
mh3g-save-convert rollback --manifest <MANIFEST>
mh3g-save-convert rollback-cec --manifest <MANIFEST>
```

Both commands require an exact converter-generated manifest; they do not accept
a save directory, backup file, or archive. With all emulators stopped:

```bash
"${CLI[@]}" rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"
"${CLI[@]}" rollback-cec --manifest "$CEMU_DIR/.cec.mh3g-install.json"
```

Rollback restores or removes only the manifest-bound target and clears that
transaction's controlled artifacts. It never changes the 3DS source.

### Runtime evidence and packages

The converter recognizes the Japanese `0x2B` profile only. Its static tables
and provenance are in `crates/mh3g-save-convert/data/catalog-provenance.json`;
the detailed data boundary is in
[`docs/adr/0013-mh3g-cross-format-conversion.md`](docs/adr/0013-mh3g-cross-format-conversion.md)
and [the exact MH3G file contract](docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md).

**macOS arm64 (isolated CLI verified 2026-07-26):** the packaged release binary
passed its inner checksum, ZIP checksum, extraction, and extracted `--help`
checks. Two real Japanese sources (`user1` and `user2`) each passed `inspect`,
`inspect-progress`, `inspect-events`, explicit dry-run with no target created,
isolated write, output readback, reinstall/backup/history creation, and
manifest-bound rollback. Both sources remained byte-for-byte unchanged; their
outputs were `0x8A24`, and written hashes matched dry-run hashes. The real
eight-file ExtData directory also passed dry-run with no output directory and
write to a fresh staging directory with all documented output sizes/hashes.
Read-only CEC inspection reported zero received inbox messages and one local
outbox message, so experimental CEC writing was not attempted. Cemu was not
started and no existing MLC was read or written; this is CLI/file validation,
not a new gameplay/runtime claim.

Build and reproduce the same package on an Apple Silicon Mac:

```bash
./scripts/package-mh3g-macos.sh artifacts/mh3g-converter
```

This creates `mh3g-save-convert-macos-arm64.zip`, its `.zip.sha256` sidecar,
and the extracted staging directory with the binary, bilingual README, and
inner binary checksum. The script never reads a save.

**Windows x64:** `.github/workflows/mh3g-converter-windows.yml` builds a native
statically linked `x86_64-pc-windows-msvc` executable, packages its checksum and
launcher, simulates Mark-of-the-Web, and runs synthetic write/rollback evidence.
The current PR's GitHub-hosted workflow result is the package-CI evidence; it
does not prove an individual PC's AppLocker, Smart App Control, antivirus,
archive-preview, or directory-permission policy. Download only a successful
workflow artifact, verify its ZIP SHA-256 sidecar, fully extract it, then use
the included `Run-Converter.ps1`. Retain the complete operation/path line if
Windows reports error 5 (`Access is denied`).


## Office Mac ↔ home Android user flow

Phase 1 alpha now uses a Chinese-first sync workbench so the user can see the
actual target instead of pressing an opaque “sync” button:

1. Run or self-host the server and use the same URL on both devices.
   - macOS: run
     `swift run --package-path apps/macos MHSaveSyncMac --set-server-url <url>`
     once, or set `MH_SAVE_SYNC_SERVER_URL` for one-off CLI sessions.
   - Android: enter the same server address in the app.
   - Current isolated Alpha test API: `http://8.130.112.207:39082`
     (server API only; MinIO/admin ports are not client endpoints).
2. macOS Nemessix before launch: install the local menu-bar app once with
   `./scripts/install-macos-app.sh`, then open `/Applications/MH Save Sync.app`.
   The app is a menu-bar utility: look for `MH 云存档` in the top-right menu bar,
   not in the Dock. First configure `设置服务器地址…`, `选择 Mac Nemessix 存档目录…`
   and `生成恢复密钥文件` (recommended) or `选择恢复密钥文件…`. The menu-bar title changes between
   `MH 云存档 · 设服务器/选目录/选密钥/就绪`, and the menu top lines always show
   `同步路线` and `下一步：...`, so after setting only the server URL it will still
   tell you to pick the save folder and generate/select the recovery-secret file before syncing. Then use `启动前检查` before MH3G,
   `立即上传 Mac 存档到服务器` for manual sync, `我已退出 MH3G：立即对账上传` after
   quitting, `查看云端状态` to see where the save went, and
   `自动同步：退出 Nemessix 后上传` if you want menu-bar exit detection.
   `新手引导：办公室 Mac ↔ 回家 Android` explains the same flow inside the app.
3. Android Nemessix before launch: authorize the Nemessix save folder, keep
   `MH3G / Android Nemessix` enabled, then tap `启动前检查`. Android shows whether
   it is uploading, downloading to the phone cache, waiting for you to exit MH3G,
   or blocked because the server address is missing.
4. If cloud and local versions differ, the app requires an explicit choice:
   `云端覆盖本地（先备份，需停止 Nemessix）` or `本地替换云端（保留云端旧版本）`.
   Both histories are retained; there is no newest-time auto overwrite.
5. If the server is unavailable, continuing local play is explicit. Local queues
   remain intact and upload resumes after the server recovers.

Player-facing Chinese guide: `docs/ux/USER_GUIDE_ZH.md`.
Engineering UX contract: `docs/ux/SYNC_USER_FLOWS.md`.
UI/UX research baseline: `docs/research/UI_UX_PATTERNS.md`.

## Repository map

- `crates/`: shared Rust domain, engine, crypto, adapters, client, server and CLI
- `apps/`: native macOS and Android shells
- `deploy/compose/`: self-hosted PostgreSQL and S3-compatible deployment
- `docs/research/`: evidence-backed research and experiments
- `docs/adr/`: accepted architecture decisions
- `docs/api/openapi-v1.yaml`: REST/OpenAPI v1 contract draft
- `scripts/`: reproducible development, backup and verification tools

## Status

Phase 1 feature branch currently contains:

- evidence-backed cloud-save, crypto/conflict, emulator-matrix, write-timeline and self-hosting research drafts;
- Rust workspace for domain, crypto, engine, adapters, client, server and CLI;
- encrypted fixed-chunk fixture snapshots, DAG conflict preservation, safe restore gating and SQLite WAL metadata tests;
- PostgreSQL + S3/MinIO persistent service with signed device certificates,
  missing-set resumable uploads, checksum fail-closed writes, transactional
  CAS HEAD commits, S3 SHA-256 upload checksums, bucket versioning init and
  readiness checks for committed object references;
- macOS Swift shell smoke, Android SAF/WorkManager/foreground-service shell,
  and generated UniFFI Kotlin/Swift bridge evidence.
- CI supply-chain gates for `cargo deny`, `cargo audit`, dependency review,
  CycloneDX SBOM generation, secret scanning and artifact checksums. The
  self-hosted-compatible PR path currently runs Rust and Android automatically.
  MHToolkit presently exposes one 2c4g `ci-general` runner, so CI cancels stale
  pushes and serializes heavy Rust → Android jobs instead of contending for the
  same host. A separate weekly `ci-canary` runs only lightweight runner/script/
  UX-copy health checks. Self-hosted Compose healthchecks default to 15s/15s/30s
  PostgreSQL/MinIO/server intervals and can be tuned with `MH_SAVE_SYNC_*_HEALTH_*`
  env vars for smaller hosts. `deploy/compose/compose.tls.yaml` adds an optional
  Caddy reverse proxy so production deployments can keep the API on loopback and
  publish only 80/443; macOS and Compose evidence remains recorded in
  `docs/runbooks/PHASE1_VALIDATION.md`.

Still not stable: real macOS↔Android↔second-emulator round trips, polished
export/import UX, upgrade/rollback benchmark, public-trusted TLS endpoint
verification and real-emulator bundle recovery remain open gates in
`docs/ROADMAP.md`. Fixture no-server bundle recovery is covered by
`scripts/offline-bundle-e2e.sh`; the isolated `mh-save-sync-aliyun` deployment,
public Alpha API gate, disaster-recovery gate and optional Caddy TLS reverse
proxy config gate are recorded in `docs/runbooks/PHASE1_VALIDATION.md`.

## Five-minute local demo

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

To install the macOS Alpha app for normal double-click usage on this Mac:

```bash
./scripts/install-macos-app.sh
open -a "/Applications/MH Save Sync.app"
```

The app menu can set the server, save directory, recovery-secret file, manual upload,
cloud status, restore and exit-after-upload automation directly. The CLI path below
writes the same persisted config if you prefer scripting:

```bash
"/Applications/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac" \
  --set-server-url http://8.130.112.207:39082
```

macOS shell can call the same Rust CLI pipeline used by Android/CLI demos:

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

Visible server sync demo (shows where the snapshot went, the logical save ID,
the cloud HEAD and conflict branch count):

```bash
MH_SAVE_SYNC_BIND=127.0.0.1:18080 cargo run -p save-server --bin mh-save-server

cargo run -p save-cli --bin mh-save -- server-upload \\
  --server-url http://127.0.0.1:18080 \\
  --root tests/fixtures/generic-save \\
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555 \\
  --device-id office-mac

cargo run -p save-cli --bin mh-save -- server-status \\
  --server-url http://127.0.0.1:18080 \\
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555
```

`server-upload` prints Chinese `message_zh`, `server_url`, `sync_target`,
`logical_save_id`, `cloud_head_before`, `cloud_head`, `outcome` and
`conflict_snapshot`, so a manual sync is never a black box. `server-status`
also includes `conflict_diffs` when the client can decrypt the current HEAD and
conflict branches with the recovery secret. Phase 1 exposes an explicit
game-specific parser contract: `mh3g-3ds` currently reports file/byte-level
differences for MH3G/3U 3DS saves and deliberately does **not** claim hunter,
equipment, item or quest semantic merges until a game-specific parser proves
those fields.

Local save-diff smoke:

```bash
cargo run -p save-cli --bin mh-save -- save-diff \\
  --left /tmp/mh-save-left \\
  --right /tmp/mh-save-right \\
  --game-profile mh3g-3ds
```

The reproducible gate is `./scripts/server-sync-e2e.sh`: it uploads an office
snapshot, uploads a home/Android-style divergent branch without a base head,
and verifies the cloud HEAD is preserved while the conflict branch is retained
and user-readable file/byte diff metadata is returned.

Automation policy gate:

```bash
./scripts/automation-policy-e2e.sh
```

This fixes the trigger contract shared by macOS and Android: file-system events
only mark dirty; save-complete, emulator-exit, periodic reconcile and manual
sync may create a stable snapshot candidate; remote restore remains blocked
while an emulator is running.

Android local build/lint, using Android Studio's bundled JBR when macOS has no
system Java:

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug
```

Install/launch smoke for the generated debug APK on exactly one connected ADB
device or emulator:

```bash
./scripts/android-apk-smoke.sh
```

The smoke installs `apps/android/app/build/outputs/apk/debug/app-debug.apk`,
launches `org.mhtoolkit.savesync/.MainActivity`, checks that it becomes the
resumed activity and fails if launch logcat contains an app crash. It does not
exercise SAF or real emulator save access; use it as the quick "can I install
the APK before going home?" gate before the shared-folder sync E2E below.

Android UI copy smoke for the same launched APK:

```bash
./scripts/android-ui-copy-smoke.sh
```

This dumps the actual Android view hierarchy and fails unless the visible app
copy explains `MH 云存档同步`, office Mac ↔ home Android, sync route, no silent
overwrite, server target, Android Nemessix folder authorization, MH3G toggle and
pre-launch check in Chinese.

Android Generic Folder shared-storage smoke with a connected ADB device:

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 \
  ./scripts/android-generic-folder-e2e.sh
```

This verifies the generic user-selected-folder path across macOS, the public
Alpha API and Android `/sdcard` shared storage, including conflict retention and
running-restore fail-closed behavior. It is not a Nemessix/Azahar/Citra runtime
claim; emulator-specific adapters still require emulator-readable restore
evidence.


Offline no-server recovery demo:

```bash
./scripts/offline-bundle-e2e.sh
```

This exports `tests/fixtures/generic-save` to an encrypted `.mhsavebundle`,
restores it to a fresh directory, byte-compares the result, and verifies that
`--emulator-state running` fails closed without writing the target. See
`docs/runbooks/OFFLINE_BUNDLE_RECOVERY.md`.

Self-hosted local demo with external secret files:

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

# Full persistent backend CLI restore gate. This starts an isolated Compose
# project on free localhost ports, uploads office/home divergent snapshots,
# keeps the cloud HEAD unchanged, restores the cloud HEAD byte-for-byte and
# verifies running-emulator restore fails closed.
CONTAINER_RUNTIME=podman ./scripts/compose-server-sync-e2e.sh
```

`scripts/compose-server-sync-e2e.sh` can also target an already running
persistent server with `MH_SAVE_SYNC_SERVER_URL=...`. When it starts Compose
it checks that the selected runtime daemon is actually usable; if Docker CLI is
installed but the daemon is down, it falls back to Podman instead of failing
mid-test. The lightweight runtime-selection guard is
`./scripts/compose-server-sync-e2e-runtime-test.sh`.

Backup and destructive restore:

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

Latest local validation evidence is summarized in
`docs/runbooks/PHASE1_VALIDATION.md`.
