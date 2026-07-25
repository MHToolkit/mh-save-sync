# MH Save Sync

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

`mh3g-save-convert` migrates one **Japanese** MH3G 3DS save slot from
Nemessix/Azahar to the matching Japanese MH3G HD Cemu slot. It is a local-only,
one-way tool: it does not upload a save, alter a source save, support another
region, or convert Cemu back to 3DS. It preserves bytes outside the documented
conversion ranges, but that byte preservation is not a semantic verification of
every in-game field.

Before **any** `--write` or `rollback`, fully quit Nemessix, Azahar, and Cemu.
Do not rely only on closing a game window; wait until their processes have
stopped. `inspect` and dry-run conversion are read-only.

Set the source and target to the same numbered slot. For the default local
Nemessix and Cemu paths, a `user2` migration is:

```bash
SOURCE="$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/00000000000000000000000000000000/00000000000000000000000000000000/title/00040000/00048100/data/00000001/user2"
CEMU_DIR="$HOME/Library/Application Support/Nemessix Dev/cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
TARGET="$CEMU_DIR/user2"

# Read only: confirm this is the supported Japanese 3DS profile.
rtk cargo run -p mh3g-save-convert -- inspect "$SOURCE"

# Read only: decode every task and compare logical completion bits by quest ID.
rtk cargo run -p mh3g-save-convert -- inspect-progress "$SOURCE" --target "$TARGET"

# Read only: compare simple story flags and categorized event coordinates.
rtk cargo run -p mh3g-save-convert -- inspect-events "$SOURCE" --target "$TARGET"

# Read only: produce and validate the result without changing Cemu's slot.
rtk cargo run -p mh3g-save-convert -- convert "$SOURCE" --output "$TARGET" --dry-run

# All emulators must be stopped. Atomically install, preserving any old target.
rtk cargo run -p mh3g-save-convert -- convert "$SOURCE" --output "$TARGET" --write

# If Cemu validation fails, with all emulators still stopped, undo that install.
rtk cargo run -p mh3g-save-convert -- rollback \
  --manifest "$CEMU_DIR/.user2.mh3g-install.json"
```

`--write` creates a same-directory backup when `user2` already exists and writes
the named manifest. Keep that manifest until you have validated the migrated
slot in Cemu or completed rollback. The converter currently recognizes the
Japanese `0x2B` profile only. File-level success is not yet a runtime
compatibility claim; Cemu load/readback and rollback evidence are required
before this repository labels the path Runtime Verified. See
`docs/adr/0013-mh3g-cross-format-conversion.md` for the conversion contract and
provenance.

The progress decoder maps the 3DS and Wii U task tables by quest ID, including
the 16 completion words at payload offset `0x6E5C`. It also converts the 58
simple-event words at `0x62AE` and preserves the byte-addressed categorized
event table at `0x668C`. Static table provenance is pinned in
`crates/mh3g-save-convert/data/catalog-provenance.json`.

### Shared extdata, guild cards, and StreetPass CEC

`card1`, `card2`, `card3`, `cardbox`, and `quest1` through `quest4` are shared
3DS extdata files, separate from `user1`/`user2`/`user3`. They must not be
treated as part of a successful slot conversion merely because `user#` loads.

The official Japanese transfer program has byte-copy states for these eight
components. `convert-extras` preserves that component payload structure and
only replaces the outer container. This is a file-level operation, not proof
that every received guild card works in the Wii U UI:

```bash
EXTRAS_SOURCE="$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/00000000000000000000000000000000/00000000000000000000000000000000/extdata/00000000/00000481/user"
EXTRAS_OUTPUT="$HOME/Desktop/mh3g-cemu-extras"

# Read only: validates every source component and reports output hashes.
rtk cargo run -p mh3g-save-convert -- convert-extras \
  --source-dir "$EXTRAS_SOURCE" \
  --output-dir "$EXTRAS_OUTPUT" \
  --dry-run

# Writes only the new output directory. Existing component files are refused.
rtk cargo run -p mh3g-save-convert -- convert-extras \
  --source-dir "$EXTRAS_SOURCE" \
  --output-dir "$EXTRAS_OUTPUT" \
  --write
```

`--reset-guild-cards` is an explicit destructive recovery option, not part of
normal migration. It replaces every `card*` payload with an empty native-Cemu
component and therefore discards both local and received guild cards. Keep the
source extdata and install only after making a backup of the Cemu destination.

StreetPass/Hunter Search data is a separate 3DS CEC mailbox. It is not stored
in `card*`, and it is not migrated by `convert-extras`. Before attempting any
future semantic CEC conversion, inspect both sides without writing:

```bash
CEC_SOURCE="$HOME/Library/Application Support/Nemessix/nand/data/00000000000000000000000000000000/sysdata/00010026/00000000/CEC/00048100"
CEMU_CEC="$CEMU_DIR/cec"

rtk cargo run -p mh3g-save-convert -- inspect-cec \
  --source-dir "$CEC_SOURCE" \
  --source-slot "$SOURCE" \
  --target "$CEMU_CEC"
```

The report gives the 3DS CEC inbox/outbox counts, validates each message
header, and reports whether the source slot's guild-card anchor occurs in an
outbox message. For the MH3G message shape (`body_size == 0x2A08`) it also
reports the read-only `0x2A00` record-shaped candidate after the observed
8-byte body prefix. Cemu stores 50 fixed-size record slots after a 0x1FC-byte
cache prefix. `convert-cec` is an **experimental raw-slot candidate**, not a
verified received-guild-card migration. It considers only records in
`InBox___`: `OutBox__` describes the source hunter's own outgoing transmission
and is intentionally never written to Cemu. It copies each non-empty received
MH3G record into the first available empty slot (or slots at/after `--slot`)
and never overwrites an existing non-empty slot:

```bash
# Read only: plan the import and print before/after hashes.
rtk cargo run -p mh3g-save-convert -- convert-cec \
  --source-dir "$CEC_SOURCE" \
  --target "$CEMU_CEC" \
  --dry-run

# All emulators must be stopped. Write atomically and keep a hash-addressed
# .cec.mh3g-backup-* plus .cec.mh3g-install.json beside the target.
rtk cargo run -p mh3g-save-convert -- convert-cec \
  --source-dir "$CEC_SOURCE" \
  --target "$CEMU_CEC" \
  --write --experimental

# If the runtime check is not useful, restore the exact pre-write cec file.
rtk cargo run -p mh3g-save-convert -- rollback-cec \
  --manifest "$CEMU_DIR/.cec.mh3g-install.json"
```

The source 3DS wrapper and its 8-byte message prefix are intentionally not
copied. The raw record placement is backed by the observed Japanese Cemu
container geometry and an isolated Cemu process-memory canary, but the
in-game guild-card list and Hunter Search UI still require runtime validation.

### Windows 11 x64 test package

The `mh3g-converter-windows` GitHub Actions workflow builds a native,
`x86_64-pc-windows-msvc` executable with Rust's CRT linked statically, runs the
converter test suite and release-binary help smoke check on Windows, then uploads
`mh3g-save-convert-windows-x64.zip` and its SHA-256 sidecar as a workflow
artifact. Download the artifact from the pull request or the merged `main`
workflow run, extract the inner ZIP, and use PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\mh3g-save-convert-windows-x64.zip
.\mh3g-save-convert.exe --help
```

The Windows executable has the same Japanese-profile-only, source-read-only,
backup, dry-run, write, and rollback requirements documented above. Do not use
`--write` while Cemu, Azahar, or Nemessix is running.


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
