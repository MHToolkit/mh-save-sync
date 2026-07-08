# Phase 1 validation evidence ledger

- Status: live evidence ledger, not a stability claim
- Last updated: 2026-07-08
- Git commit when captured: this branch state; exact commit reported in PR/final status
- Secret policy: commands below use external secret files under
  `~/Documents/Secrets`; no recovery phrase, access token, device secret,
  plaintext save bytes or user save path content is recorded here.

## Local gates executed

```text
cargo fmt --all -- --check                                      PASS
cargo test --workspace                                          PASS: includes automation policy tests
cargo clippy --workspace --all-targets -- -D warnings           PASS
cargo build --workspace --bins                                  PASS
scripts/supply-chain-gate.sh                                    PASS: cargo-deny + cargo-audit + CycloneDX SBOM
cargo run -p save-cli --bin mh-save -- crypto-device-fixture    PASS: matches tests/fixtures/device-identity-public.json
cargo test -p save-cli --test bundle_cli                       PASS: 2 tests / 1 suite
cargo test -p save-cli --test server_sync_cli                  PASS: 2 tests / 1 suite
scripts/offline-bundle-e2e.sh                                   PASS: export bundle, restore, running fail-closed
scripts/server-sync-e2e.sh                                      PASS: upload/status/restore, conflict branch retained
scripts/macos-shell-e2e.sh                                      PASS: macOS shell upload/status/restore visible
scripts/automation-policy-e2e.sh                                PASS: watcher dirty-only, session-boundary snapshot candidates, running restore blocked
scripts/macos-install-e2e.sh                                    PASS: local .app install, bundled CLI, persisted server URL, save root, recovery secret file, manual/auto menu labels
scripts/android-avd-generic-folder-e2e.sh                    PASS: headless AVD shared-storage public Alpha conflict/restore gate
scripts/compose-server-sync-e2e-runtime-test.sh                 PASS: Docker daemon failure falls back to Podman
scripts/compose-server-sync-e2e.sh                              PASS: postgres-s3 upload/status/restore and conflict branch
scripts/compose-project-volume-test.sh                          PASS: backup/restore use isolated Compose project volumes
cargo build --release -p save-client                            PASS
UniFFI Kotlin binding generation                                PASS
UniFFI Swift binding generation                                 PASS
Android assembleDebug testDebugUnitTest lintDebug               PASS
podman compose up -d --build --wait                             PASS
scripts/compose-e2e.py                                          PASS
scripts/compose-resume-e2e.py prepare/restart/finish            PASS
deploy/compose/scripts/backup.sh                                PASS
deploy/compose/scripts/restore.sh                               PASS
```

The 2026-07-05 supply-chain pass upgraded the server stack away from
`sqlx 0.8.0` and the old AWS SDK TLS chain. `cargo-deny` and `cargo-audit`
now pass with two reviewed temporary ignores for `quick-xml` advisories
`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`, both transitive through
`object_store 0.14` S3 response XML parsing. The server does not parse
user-provided XML; the ignore must be removed when object_store releases a
quick-xml `>=0.41` update.


## macOS menu-bar sync UX gate executed on 2026-07-08

Commands:

```bash
swift build --package-path apps/macos
./scripts/build-macos-app-bundle.sh
./scripts/macos-install-e2e.sh
./scripts/macos-config-e2e.sh
```

Evidence scope:

- The menu-bar title now exposes setup progress as `MH 云存档 · 设服务器/选目录/选密钥/就绪`, and the menu top exposes `同步路线：MH3G / macOS Nemessix → 本机安全缓存 → <server>` plus `下一步：...` after every launch/config change.
- The menu-bar app now exposes player-facing Chinese actions for `打开同步向导（告诉我下一步）`, `设置服务器地址…`, `选择 Mac Nemessix 存档目录…`, `选择恢复密钥文件…`, `启动前检查`, `立即上传 Mac 存档到服务器`, `我已退出 MH3G：立即对账上传`, `查看云端状态`, `云端覆盖本地（先备份，需停止 Nemessix）`, and `自动同步：退出 Nemessix 后上传`.
- `--menu-preview` is now covered by macOS E2E scripts, so CI can verify that the visible menu explains sync destination, manual sync, auto sync and next action without launching the GUI. This closes the observed UX gap where a user configured only the server URL but could not tell that save-folder selection, recovery-secret-file selection, pre-launch check, manual upload or exit-after-upload were the next actions.
- The app bundle includes `Contents/MacOS/mh-save`; installed menu actions therefore use the same Rust CLI pipeline without requiring an external `MH_SAVE_SYNC_CLI` environment variable.
- Config E2E verifies persisted server URL, save-root path, recovery-secret file path under `~/Documents/Secrets`, and `auto_upload_on_exit=false`; the recovery secret contents are never written into config, logs, docs or GitHub output.
- Automation remains session-boundary based: the menu bar checks Nemessix process exit every 15 seconds and only then triggers stable snapshot upload; file changes do not upload directly, and restore stays blocked while Nemessix is running.

## Offline bundle recovery gate executed on 2026-07-07

Command:

```bash
./scripts/offline-bundle-e2e.sh
```

Sample local output shape from `verify-local.sh`:

```json
{"bundle":"artifacts/offline-bundle/generic-save.mhsavebundle","bundle_sha256":"170313800b2d91e50b5511afce4ec17e6b25cae0a37fc769bb269371e8bb3def","offline_bundle_restore":true,"restored_snapshot_id":"529bb8aab8fb6a61379134413ce11ddc1e6f6e9ac3d08ee2496dc1607cdacbb7","running_restore_fail_closed":true,"snapshot_id":"529bb8aab8fb6a61379134413ce11ddc1e6f6e9ac3d08ee2496dc1607cdacbb7"}
```

This is synthetic fixture evidence for no-server `.mhsavebundle` recovery.
It proves encrypted bundle export/import, byte-identical restore, and
running-emulator restore refusal. Snapshot and bundle hashes change on each run
because encrypted manifests/chunks use fresh AEAD nonces; the stable invariant is
that restore output byte-compares equal to `tests/fixtures/generic-save` and
running-emulator restore writes no target directory. It does not upgrade emulator
adapters to `RuntimeVerified`; real emulator-readable restore evidence is still
tracked as an open phase gate below.

## Visible server sync CLI gate executed on 2026-07-07

Command:

```bash
./scripts/server-sync-e2e.sh
```

Sample local output:

```json
{"server_url": "http://127.0.0.1:51699", "cloud_head": "f26039142ba42a5b69e5db96ea68a1aa34419e0415453ac1d7b27be018fe49a3", "history_count": 2, "conflict_count": 1, "restored_snapshot_id": "f26039142ba42a5b69e5db96ea68a1aa34419e0415453ac1d7b27be018fe49a3", "running_restore_fail_closed": true, "evidence": "server sync e2e preserved conflict branch and restored cloud head"}
```

This is synthetic server/API evidence for the manual sync UX path. It starts
the memory backend server, uploads an office/macOS-style snapshot, uploads a
home/Android-style divergent snapshot without a base head, and verifies that
the original cloud HEAD remains unchanged while the divergent snapshot is
retained as a conflict branch. It then downloads/restores the cloud HEAD and
verifies running-emulator restore fails closed. CLI JSON includes `server_url`,
`sync_target`, `logical_save_id`, `cloud_head_before`, `cloud_head`, `outcome`,
`conflict_snapshot`, restored `snapshot_id` and Chinese `message_zh`, so manual
sync is not a black box. It proves memory-backend encrypted object
download/restore; persistent PostgreSQL/S3 download/restore is covered by the
next gate. It does not yet prove real emulator readability. In this Codex managed sandbox, loopback bind is denied,
so `./scripts/server-sync-e2e.sh` exits 0 with an explicit skip message unless
run outside the sandbox or in CI. CI sets `MH_SAVE_SYNC_REQUIRE_NETWORK_E2E=1`,
so the same script is a hard failure if the loopback server cannot start.

## Persistent PostgreSQL/S3 CLI restore gate executed on 2026-07-07

Commands:

```bash
./scripts/compose-server-sync-e2e-runtime-test.sh
./scripts/compose-server-sync-e2e.sh
```

Runtime probe output:

```json
{"runtime_probe_test":true,"docker_daemon_failure_falls_back_to_podman":true,"explicit_unusable_runtime_exits_77":true}
```

Persistent backend output:

```json
{"backend":"postgres-s3","cloud_head":"d533c91617fe233babbc9b386a3b24909a2eff168e342c0c59e20b6571db5256","conflict_count":1,"evidence":"persistent postgres-s3 server-upload/status/server-restore preserved conflict branch and restored byte-identical cloud HEAD","history_count":2,"logical_save_id":"compose-cli-1783414673034858000","restored_snapshot_id":"d533c91617fe233babbc9b386a3b24909a2eff168e342c0c59e20b6571db5256","running_restore_fail_closed":true,"server_url":"http://127.0.0.1:62088"}
```

This starts an isolated Compose project on free localhost ports, creates
ephemeral secret files under `~/Documents/Secrets`, verifies `/ready` reports
`backend=postgres-s3`, bootstraps the deterministic public device fixture,
uploads an office/macOS-style encrypted snapshot, uploads a divergent
home/Android-style encrypted snapshot, confirms the cloud HEAD is unchanged
while the divergent snapshot is retained as a conflict branch, downloads and
restores the cloud HEAD byte-for-byte, and verifies running-emulator restore
fails closed without creating the target directory.

The heavy persistent gate is manual/local evidence because MHToolkit currently
has one 2c4g self-hosted runner. CI runs the lightweight runtime-selection test
so Docker CLI without a daemon falls back to Podman instead of failing in the
middle of Compose startup.

## Compose project-aware backup/restore gate executed on 2026-07-07

Command:

```bash
./scripts/compose-project-volume-test.sh
```

Output:

```json
{"compose_project_volume_test":true,"project":"mh-save-sync-aliyun","isolated_volumes":true}
```

Remote preflight for `8.130.112.207` found no current conflict for Compose
project `mh-save-sync-aliyun` or host ports `18082/19082/19083`, but it also
found that backup/restore previously hard-coded default `mh-save-sync_*` volume
names. `deploy/compose/scripts/backup.sh`, `restore.sh` and
`verify-repository.sh` now pass `--project-name` and derive PostgreSQL/MinIO
volume names from `COMPOSE_PROJECT_NAME` (or `MH_SAVE_SYNC_COMPOSE_PROJECT`) so
isolated remote backup/restore targets only the intended project. The lightweight
test uses a fake runtime and fails if default project volumes leak back in.

## Aliyun isolated deployment gate executed on 2026-07-07

Scope:

- Host: `8.130.112.207`
- Compose project: `mh-save-sync-aliyun`
- Remote app dir: `/home/ecs-user/mh-save-sync-bc6b2de`
- Secret material: read from the remote env file only; no secret value is
  recorded here.
- Boundary: no `nemessix-room` or other Compose project was stopped, removed or
  reused.

Deployment notes:

- The server container is intentionally isolated under project
  `mh-save-sync-aliyun`.
- Remote Docker build initially reused an older server image that returned 404
  for `/v1/snapshots/{snapshot_id}/encrypted-bundle`. RCA found the remote
  Compose `server` service had `image:` but no `build:` stanza, so
  `docker compose build server` was a no-op.
- The remote executor rebuilt the current server image explicitly from
  `deploy/compose/server.Dockerfile` using a vendored/offline cargo build path.

Evidence:

```text
server image: sha256:37c3efd737ddd737238efcb348cb71cd4da6bc992f53a5dad4d3b5d3bbc5ebaa
containers: mh-save-sync-aliyun-server-1/postgres-1/minio-1 all healthy
server bind: 127.0.0.1:18082->8080/tcp
minio bind: 0.0.0.0:19082->9000/tcp, 0.0.0.0:19083->9001/tcp
ready: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
route validation: /v1/snapshots/nothex/encrypted-bundle returns HTTP 400, proving the current route is present
```

Remote persistent sync was verified through an SSH tunnel to the isolated
loopback port:

```json
{"backend":"postgres-s3","cloud_head":"389bf1e3d21f8884fa19bd87276c3fa0f39d008a8674cf5af6f8d4e81f1e63a4","conflict_count":1,"evidence":"persistent postgres-s3 server-upload/status/server-restore preserved conflict branch and restored byte-identical cloud HEAD","history_count":2,"logical_save_id":"compose-cli-1783423011431965000","restored_snapshot_id":"389bf1e3d21f8884fa19bd87276c3fa0f39d008a8674cf5af6f8d4e81f1e63a4","running_restore_fail_closed":true}
```

Remote disaster recovery:

```text
backup: /home/ecs-user/Games/Backups/MHSaveSync/mh-save-sync-aliyun-20260707-191611
postgres.sql: SHA256SUMS present and verified during restore
minio-data.tar: SHA256SUMS present and verified during restore
restore scope: recreated only mh-save-sync-aliyun_postgres-data and mh-save-sync-aliyun_minio-data
post-restore ready: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
post-restore repository check: dangling_snapshot_objects=0
post-restore logical save: compose-cli-1783422931281127000 history_count=2 conflict_count=1
```

Public Alpha API entrypoint:

```text
public server_url: http://8.130.112.207:39082
ready: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
```

The public API is currently exposed through a minimal TCP proxy on the isolated
test instance, forwarding `0.0.0.0:39082` to the local Compose server on
`127.0.0.1:18082`. MinIO ports `19082/19083` are not part of the public client
contract; clients only need the API URL above.

Public persistent sync was verified against the same URL that Android/macOS
clients can configure:

Command:

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 ./scripts/compose-server-sync-e2e.sh
```

Evidence:

```json
{"backend":"postgres-s3","cloud_head":"8054a06dc92c245703130b320539e34f13dfce04a9e0ca8dcc76311187426d30","conflict_count":1,"evidence":"persistent postgres-s3 server-upload/status/server-restore preserved conflict branch and restored byte-identical cloud HEAD","history_count":2,"logical_save_id":"compose-cli-1783424652296908000","restored_snapshot_id":"8054a06dc92c245703130b320539e34f13dfce04a9e0ca8dcc76311187426d30","running_restore_fail_closed":true,"server_url":"http://8.130.112.207:39082"}
```

Known deployment boundary:

- Direct public `http://8.130.112.207:18082/ready` currently times out from the
  local network; the public alpha API uses `39082` instead.
- `39082` packet capture showed public TCP handshake, HTTP request and 200 OK
  response on the ECS private interface, so the user-facing API path is open.
- The `39082` proxy is an alpha-test convenience, not the final production
  ingress design. Production deployments should use `deploy/compose/compose.tls.yaml`
  with Caddy or an equivalent managed reverse proxy/load balancer TLS endpoint,
  bind `MH_SAVE_SYNC_HTTP_PORT` to `127.0.0.1:<port>`, and keep Compose internals
  private. A public-trusted certificate still requires a real DNS name pointed
  at the host; the current IP-only Alpha endpoint remains HTTP.

TLS reverse-proxy config gate executed on 2026-07-08:

Command:

```bash
./scripts/compose-tls-config-test.sh
```

Evidence:

```json
{"compose_tls_config_test":true,"static_files_checked":true,"compose_config_checked":true,"tls_proxy":"caddy","public_ports":[80,443],"upstream":"server:8080"}
```

The gate verifies `deploy/compose/Caddyfile`, `deploy/compose/compose.tls.yaml`,
loopback API binding projection (`127.0.0.1:18082`), public 80/443 projection,
Caddy persistent volumes, TLS proxy healthcheck tunables and security headers.
It does not claim the current `8.130.112.207:39082` Alpha URL has a trusted TLS
certificate; DNS-backed deployment remains required before marking production
TLS fully verified.


## macOS shell server sync gate executed on 2026-07-07

Command:

```bash
./scripts/macos-shell-e2e.sh
```

Sample local output:

```json
{"server_url": "http://127.0.0.1:58737", "macos_shell_upload_visible": true, "macos_shell_status_visible": true, "macos_shell_restore_visible": true, "running_restore_fail_closed": true}
```

This proves the macOS SwiftPM shell can invoke the shared Rust `mh-save` CLI for
server upload, server status and stopped-emulator restore while preserving the
running-emulator fail-closed guard. It is still an unsigned Alpha shell; the
local `.app` install path is covered separately below and does not yet claim
notarization or LaunchAgent deployment.

## UX correction gates executed on 2026-07-07

```text
cargo fmt --all -- --check                                      PASS
git diff --check                                                PASS
cargo test --workspace                                          PASS: includes automation policy tests
cargo clippy --workspace --all-targets -- -D warnings           PASS
Android assembleDebug testDebugUnitTest lintDebug               PASS
swift build --package-path apps/macos                           PASS
scripts/build-macos-app-bundle.sh                               PASS: generated local double-clickable .app bundle
scripts/macos-config-e2e.sh                                      PASS: persisted server URL under Application Support
scripts/macos-install-e2e.sh                                     PASS: installed app copy reads persisted server URL
swift run --package-path apps/macos MHSaveSyncMac --status      PASS
swift run --package-path apps/macos MHSaveSyncMac --prelaunch-check PASS
swift run --package-path apps/macos MHSaveSyncMac --conflict-demo PASS
scripts/secret-scan.sh                                          PASS
scripts/artifact-checksums.sh Android APK + macOS executable    PASS
```

UX correction scope:

- Android app label and workbench are Chinese-first for phase1 alpha.
- Android now shows the server destination and full route
  `MH3G / Android Nemessix -> local staging/CAS -> server`, per-game enable
  switch, SAF authorization, explicit conflict choices, manual upload,
  download-to-cache-only, restore-cloud-to-local with stopped-emulator
  precondition, active game-protection state and visible background reconcile
  summaries.
- Android pre-launch check now probes the configured server `/ready` and the
  MH3G Nemessix logical-save HEAD
  `243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2`.
  Cloud-unavailable, no-server, no-remote-head and remote-head states are
  visible before launch; package visibility for Nemessix is declared so
  `检查后打开 Nemessix` can fail with an explicit message instead of looking
  broken.
- Android foreground notification now states that running sessions are game
  protection sessions: they forbid cloud overwrite and reconcile only after
  exit.
- Android active-session button copy now uses player language
  `我正在玩 MH3G（保护本地存档）` / `我已退出 MH3G（开始对账上传）`
  instead of internal lock/session jargon; `SyncMessagesTest` asserts these
  strings, the running-restore refusal and the Android notification channel do
  not regress to `锁定`, `标记会话` or `同步会话`.
- macOS SwiftPM smoke keeps CI-friendly CLI mode, adds `--app` menu-bar shell
  with in-app server URL setting, status, pre-launch check, conflict and
  cloud-unavailable actions.
  `scripts/build-macos-app-bundle.sh` builds the local
  `artifacts/macos/MH Save Sync.app`, while `scripts/install-macos-app.sh`
  copies it into an Applications-style directory for normal double-click use.
- macOS now supports `--set-server-url <url>`, persisted at
  `~/Library/Application Support/MH Save Sync/config.json`. The menu-bar app
  and CLI status/pre-launch paths read this config when `MH_SAVE_SYNC_SERVER_URL`
  is not set, so office Mac and home Android can both point at the same server
  without requiring a shell environment.
- Shared Rust client exposes Chinese launch-gate/conflict decision records for
  future UniFFI UI wiring and tests cloud-unavailable, remote-newer and conflict
  behavior without last-write-wins.
- Final pass also fixes the local `scripts/secret-scan.sh` empty-untracked
  false positive introduced by the runner-migration update, so secret scanning
  remains fail-closed for real matches without failing on an empty local list.

Artifact hashes from this correction:

```text
Android debug APK:
a87004d3edb1892a469bb8036dccd503c2d201a767f1caf68253a78db793c9ea  apps/android/app/build/outputs/apk/debug/app-debug.apk

macOS smoke executable:
7eb3ae13c543b5171e22cddbf5acffd1891795f1ed7c078aa8ddb5c782f02e48  apps/macos/.build/debug/MHSaveSyncMac

macOS local app executable:
7eb3ae13c543b5171e22cddbf5acffd1891795f1ed7c078aa8ddb5c782f02e48  artifacts/macos/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac
```

## Android state-first primary-action gate executed on 2026-07-08

Commands:

```text
python3 scripts/ux-copy-guard.py                                PASS
Android testDebugUnitTest lintDebug assembleDebug               PASS
scripts/secret-scan.sh                                          PASS
git diff --check                                                PASS
```

Scope:

- Android first screen now surfaces one primary recommended action inside
  `当前状态和下一步`, so the user does not need to understand internal queue,
  storage or locking terms before acting.
- The primary action changes by safe state: open MH3G sync, choose Android
  Nemessix folder, fill the shared server address, mark `我已退出 MH3G`, or run
  `启动前检查`.
- The same card explains why the action is safe: no upload before server/folder
  setup, folder selection does not immediately upload or overwrite, launch check
  does not modify local saves, and running sessions still forbid cloud overwrite.
- `SyncMessagesTest.dashboardSummaryExplainsCurrentStateAndNextAction` now
  locks the Chinese first-screen CTA copy and rejects internal terms such as
  `锁定`, `标记会话`, `同步会话`, `SAF`, `CAS`, `HEAD`, `dirty` and `watcher`.

This is Android client UX evidence only. It does not upgrade any emulator
descriptor to `RuntimeVerified`; real emulator-readable restore evidence remains
tracked as an open phase gate below.

## Automation trigger policy gate executed on 2026-07-08

Command:

```bash
./scripts/automation-policy-e2e.sh
```

Output:

```json
{"automation_policy_e2e":true,"remote_download_live_overwrite":false,"running_restore_fail_closed":true,"session_boundary_events":["save-complete","emulator-exit","periodic-reconcile","manual-sync"],"stable_snapshot_required_before_upload":true,"watcher_event":"dirty-only-no-upload"}
```

This gate targets the shared Rust client automation contract used by macOS and
Android shells. It proves that FSEvents/FileObserver-style dirty observations
only set a dirty flag and never upload directly, while `save-complete`,
`emulator-exit`, periodic reconcile and manual sync are the only events that
create a stable snapshot candidate. It also proves remote content may download
to local CAS while an emulator is running, but live overwrite restore remains
blocked until the emulator stops. The script asserts each targeted Rust test ran
with `1 passed` to avoid false-green filtered test output.

## macOS persisted server configuration gate executed on 2026-07-08

Command:

```bash
./scripts/macos-config-e2e.sh
```

Output:

```json
{"config_path":"~/Library/Application Support/MH Save Sync/config.json","macos_config_e2e":true,"server_url":"http://127.0.0.1:39082"}
```

This runs the Swift shell with an isolated `HOME`, saves a server URL through
`--set-server-url`, verifies `--status` and `--prelaunch-check` read the same
persisted server URL without `MH_SAVE_SYNC_SERVER_URL`, and inspects the JSON
config file for `server_url`. It proves the local `.app` can show the same
sync destination after double-click launch instead of depending only on a shell
environment variable.

## macOS local app install gate executed on 2026-07-08

Command:

```bash
./scripts/macos-install-e2e.sh
```

Output:

```json
{"install_dir":"/tmp/.../Applications","installed_app":"/tmp/.../Applications/MH Save Sync.app","macos_install_e2e":true,"server_url_persisted":"http://127.0.0.1:39082"}
```

This proves the macOS app is not only an artifact under `artifacts/`:
`scripts/install-macos-app.sh` builds the menu-bar `.app`, copies it into an
Applications-style directory, validates `Info.plist`, runs the installed
executable, and confirms `--set-server-url` persists the same server destination
used by status and pre-launch checks. Source inspection confirms the menu-bar
`设置服务器地址…` action calls the same `persistServerURL` helper; this is a
source-level check, not a GUI automation claim. The default manual install
target is `/Applications/MH Save Sync.app`; automated verification uses
`MH_SAVE_SYNC_INSTALL_DIR` with a temporary directory so it does not mutate the
host app folder. The install e2e also rejects unsafe install dirs and invalid
app bundle names before any `rm -rf` destination cleanup.

Host install evidence on this Mac:

```json
{"macos_app_installed":true,"path":"/Applications/MH Save Sync.app","display_name":"MH 云存档","launch":"open -a '/Applications/MH Save Sync.app'"}
```

The installed app was configured to the public Alpha API and verified with:

```text
/Applications/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac --status
同步到服务器：http://8.130.112.207:39082
```

Public API readiness at capture time:

```json
{"status":"ready","version":"0.1.0","backend":"postgres-s3"}
```


## Android restore UX message gate executed on 2026-07-07

Command:

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  ./apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug --no-daemon
```

Evidence:

```text
SyncMessagesTest.restoreCloudHeadMessageExplainsStoppedPreconditionAndBackup PASS
SyncMessagesTest.runningRestoreMessageFailsClosedWithoutOverwrite PASS
SyncMessagesTest.syncRouteExplainsServerAndLocalCas PASS
SyncMessagesTest.cloudActionWithoutServerExplainsWhyNothingUploaded PASS
SyncMessagesTest.cloudUnavailableLaunchPauseRequiresExplicitLocalChoice PASS
SyncMessagesTest.prelaunchProbeUsesStableMh3gLogicalSaveIdAndNormalizesServer PASS
```

This proves the Android Chinese workbench exposes a restore-cloud-head action
with the stopped-Nemessix precondition and backup language, and that running
restore is visibly refused without overwriting local saves. It is still UI/state
evidence; real Android SAF byte-for-byte restore against a live Nemessix save
root remains an open Runtime Verified gate.

## Android AVD Chinese UI smoke executed on 2026-07-07

Device:

```text
AVD: Pixel_9_API_36_Daily
adb: emulator-5554 device sdk_gphone64_arm64
APK: apps/android/app/build/outputs/apk/debug/app-debug.apk
```

Evidence commands:

```bash
adb install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n org.mhtoolkit.savesync/.MainActivity
adb exec-out screencap -p > artifacts/android-ui-initial.png
```

Screenshots captured:

```text
b81739e89ecfd1363038d3cc4f75a3a90e0ddf7611c4e549d8e5befd4966bb67  artifacts/android-ui-initial.png
363fc2c3b56a101a383b398c80f25ec045df75c0cf5af5027de0fc9c5b2fd981  artifacts/android-ui-public-server.png
b03dbe7ccd7f144e24b6f49acb96b34f7739ff279dba7d6b89fb6ea54ea089c3  artifacts/android-ui-prelaunch-remote-ok.png
579f507f6c344cf7ae3821f6d2d12e578804bfbe3036526b81061ea24138a449  artifacts/android-ui-conflict-dialog.png
```

What the screenshots prove:

- Initial launch is Chinese-first and explains that office Mac and home Android
  sync to the same server instead of an opaque button.
- Configured server view shows the route
  `MH3G / Android Nemessix -> local staging/CAS -> server`, the per-game switch,
  and SAF directory state.
- Pre-launch check was executed from the Android app through `adb reverse` to an
  SSH tunnel targeting the remote Alpha API. The UI showed cloud reachable and
  no MH3G cloud HEAD, so starting local play is explicit and later upload is
  queued.
- Conflict dialog lists local/cloud device, time, parent and size, and offers
  explicit `本地替换云端` / `云端覆盖本地` choices instead of latest-time overwrite.

Boundary:

- This is Android UI/network smoke evidence only. The SAF URI used for the smoke
  is synthetic to unlock the UI path; it does not prove byte-for-byte restore
  into a real Nemessix save root and does not upgrade Android Nemessix to
  `RuntimeVerified`.
- The AVD could not consistently reach the public IP directly from its guest
  network, so the pre-launch smoke used `adb reverse tcp:39082` plus an SSH
  tunnel. Public client reachability is separately proven by the `39082` server
  gate above.
- Android `usesCleartextTraffic=true` is enabled for this Alpha because
  self-hosted users commonly start with `http://IP:port`. Save contents remain
  protected by application-layer E2EE; production deployment should use a TLS
  reverse proxy and can later tighten cleartext policy.

## Android Generic Folder shared-storage E2E executed on 2026-07-08

Commands:

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 \
  ./scripts/android-avd-generic-folder-e2e.sh
```

The wrapper starts Android Studio AVD `Pixel_9_API_36_Daily` headlessly with
`-no-snapshot-load` and `-gpu swiftshader_indirect`, waits for
`sys.boot_completed=1`, runs the shared-storage E2E, then shuts the emulator
down. The underlying direct-device command remains:

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 \
  ./scripts/android-generic-folder-e2e.sh
```

Latest wrapper output:

```json
{"adb_device":"emulator-5554","android_conflict_snapshot":"e9ab320f2d3b436779b3a983b3206fd9a350c93864e089708cb258d00f8056d5","android_generic_folder_e2e":true,"backend":"postgres-s3","cloud_head":"00dee8f6675d5a7c2639096916889f2ceecfe20a17698fd00b46a9a1e470a8a7","conflict_count":1,"history_count":2,"logical_save_id":"adb-generic-folder-1783498976075623000","restored_android_path":"/sdcard/MHSaveSyncE2E/restored-head/slot1/main.bin","restored_sha256":"d92bf81eb5f71918292b1c5515792135574123c8c98c52da0a242492e3703268","restored_snapshot_id":"00dee8f6675d5a7c2639096916889f2ceecfe20a17698fd00b46a9a1e470a8a7","running_restore_fail_closed":true,"server_url":"http://8.130.112.207:39082","support_level":"Generic Folder Android shared-storage evidence only; does not upgrade emulator-specific adapters to RuntimeVerified"}
```

What this proves:

- A real Android Studio AVD shared-storage tree under `/sdcard/MHSaveSyncE2E`
  can participate in the same PostgreSQL/S3 server protocol as the macOS
  Generic Folder flow.
- A macOS Generic Folder snapshot became cloud HEAD, an Android shared-storage
  divergent branch uploaded without a base head became a conflict branch, and
  cloud HEAD remained unchanged (`history_count=2`, `conflict_count=1`).
- Restoring the cloud HEAD while stopped produced bytes that were pushed back to
  Android shared storage and pulled back for byte comparison; the restored file
  sha256 was `d92bf81eb5f71918292b1c5515792135574123c8c98c52da0a242492e3703268`.
- Running-emulator restore still failed closed and did not create the blocked
  target directory.
- The public Alpha API occasionally returned empty/closed connections during this
  run; the script now uses bounded, low-frequency retries for `/ready`,
  bootstrap/register and CLI server operations instead of high-frequency polling.

Boundary:

- This is a Generic Folder Android shared-storage E2E, not an emulator-specific
  proof. It does not prove Nemessix/Azahar/Citra can read the restored bytes
  after relaunch and therefore does not upgrade those adapters to
  `RuntimeVerified`.
- The script uses ADB for evidence capture and cleanup. The production Android
  app must still use SAF persistable URI grants and fail closed when a user does
  not authorize the target tree.

## Self-hosted runner throttling check executed on 2026-07-07

Command:

```bash
gh api orgs/MHToolkit/actions/runners --paginate \
  --jq '.runners[] | {name,os,status,busy,labels:[.labels[].name]}'
```

Evidence rechecked on 2026-07-08:

```json
{"busy":false,"labels":["self-hosted","Linux","X64","ecs","ci-general","linux-x64","cn-hangzhou","2c4g","mhtoolkit"],"name":"ecs-cn-hangzhou-mhtoolkit-01","os":"Linux","status":"online"}
```

Adopted CI policy:

- keep workflow-level `cancel-in-progress: true` so stale pushes do not consume
  the self-hosted host;
- serialize heavyweight jobs by making Android depend on Rust, because the
  organization currently has one 2c4g `ci-general` runner;
- skip the heavy PR workflow for documentation-only updates via `paths-ignore`
  in `.github/workflows/ci.yml`; use `workflow_dispatch` or a code/config touch
  if a docs-only change explicitly needs a full integration rerun;
- keep `ci-canary.yml` weekly only (`17 4 * * 1`) as a low-frequency runner
  health check instead of frequent polling;
- avoid high-frequency status watching during development; use single
  `gh pr checks` / `gh run list` snapshots after pushes and wait between checks.

Capacity note for the MHToolkit hub: raising concurrency above 1 is not useful
while the org exposes a single 2c4g `ci-general` runner. Add a second runner or
upgrade the host before removing the Rust→Android serialization.

Self-hosted Compose load note: default healthchecks are intentionally modest for
the same small-host profile. PostgreSQL and MinIO default to 15s intervals, the
server defaults to 30s, and operators can tune `MH_SAVE_SYNC_*_HEALTH_*` values
without editing the Compose file if the host is slower or shared.

## Artifact hashes

```text
Rust debug binaries:
7345d7b2f1fa0b234816bd89772e8df7688e4724a4f661fc2a6faaeb0d4b2bcf  target/debug/mh-save
3065dd98b545347d3b3446742642299b3703eb3a45789e8116ae9daedd60d3a8  target/debug/mh-save-server

CycloneDX SBOM:
6a18f97b6c9a2e5040da02081e4d7403b2e20b13cb13b4fe43dfc2fbed75517b  artifacts/sbom/mh-save-sync.cdx.json

Android debug APK:
a87004d3edb1892a469bb8036dccd503c2d201a767f1caf68253a78db793c9ea  apps/android/app/build/outputs/apk/debug/app-debug.apk

Rust client cdylib:
0f28c63cea7d46490044b919aa1705cbd8603b5c91c72dc64760d1782cf961f6  target/release/libsave_client.dylib

Generated UniFFI bindings:
dd579e3f4b47cfbd8e91d326b55be2f72cff3a74ec34faee9227faceec99edc8  artifacts/uniffi/kotlin/uniffi/save_client/save_client.kt
6f9d6af05b44b02cd72d69e22ed9448c1f76732ddf06f2989d3ba0823d2cb9b1  artifacts/uniffi/swift/save_client.swift
eec32706d026d26b8c08eae4d83757d59b0faf4403ed14140988b692fe073885  artifacts/uniffi/swift/save_clientFFI.h
2fb10eea39f366ef73ec22e7d2407dc3167e0f7ee2147281931d3ab64b58a40c  artifacts/uniffi/swift/save_clientFFI.modulemap

Destructive restore backup:
7d4b439072fd79fd9ad012dee9b1eba589140b5857381116d66b4c47c6f0f7f3  ~/Games/Backups/MHSaveSync/20260705-124110/postgres.sql
b1322d19dcd6eaab71ae8e31b7af77a02ba6fc4db6cd72c6c12929f02bd7163f  ~/Games/Backups/MHSaveSync/20260705-124110/minio-data.tar
```

`artifacts/` and build outputs are intentionally ignored by Git. Regenerate
them with the commands above instead of committing generated binaries.

## Self-hosted black-box API evidence

Readiness after restore:

```json
{"status":"ready","version":"0.1.0","backend":"postgres-s3"}
```

`scripts/compose-e2e.py` result:

```json
{
  "account_root_immutable": true,
  "backend": "postgres-s3",
  "certificate_fail_closed": true,
  "checksum_fail_closed": true,
  "conflict_count": 1,
  "dedupe_missing_count": 1,
  "head": "f0af6da718cbddc3096e433952dc18785f01b3f29d990b8490d0fd517ef2342d",
  "history_count": 3,
  "logical_save_id": "e2e-1783226459630103000"
}
```

Restart-resume probe:

```json
{
  "head": "119beee8ef738ddf81cceba508a7ef8801b6e5cc572e9ecf44302bfc43e20fc1",
  "logical_save_id": "resume-1783226459627437000",
  "resumed_after_restart": true
}
```

Destructive restore verification:

```text
postgres.sql: OK
minio-data.tar: OK
ready: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
dangling_snapshot_objects=0
```

## Container evidence

```text
Server image id:     31d1fd65cfab353434525dbc873f32b28b779e266bc76aa41f7de828bc8b51e4
PostgreSQL image id: 5db836939fe3760739047801b3e588e97c8774d02807db98d6e977ec6a5e54a6
MinIO manifest:      sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e
MinIO arm64 image:   8f08aee614800a237906bd48114d733e5ac5bfac4ccdf731f141b0e880d7a253
SQLx migration:      1
Object store client: object_store 0.14 with S3 SHA256 upload checksum; Compose minio-init creates and versions the bucket before API start.
Post-restore sample: prior run server 0.13% CPU / 2.044 MiB RSS; PostgreSQL 1.70% / 52.18 MiB; MinIO 1.56% / 73.85 MiB
```

## Runtime support boundary

This evidence proves the shared engine/server/Android shell/self-hosting
fixtures listed above. It does **not** upgrade any path-only emulator entry to
`Runtime Verified`. Runtime Verified still requires reproducible real emulator
save/read evidence in `docs/research/EMULATOR_SAVE_MATRIX.md`.

Additional runtime boundary check on 2026-07-08:

- AVD `emulator-5554` (`sdk_gphone64_arm64`, Android 16 / API 36) was booted
  without clearing data and confirmed `/sdcard` access. It had no installed
  `nemessix`, `azahar`, `citra`, `ppsspp`, `dolphin`, `nethersx2` or `pcsx2`
  package and no corresponding shared-storage root, so it can only support the
  Generic Folder evidence above.
- The same AVD was used for an Android UI smoke after installing the current
  debug APK. `uiautomator` verified the visible Chinese sync-action surface
  includes `同步动作`, `本地替换云端（保留云端旧版本）` and
  `云端覆盖本地（先备份，需停止 Nemessix）`. A follow-up install over an
  existing app state verified persisted legacy user-copy cleanup:
  `{"android_legacy_copy_sanitized":true}`. This is UI/wording evidence only,
  not emulator Runtime Verified save evidence.
- macOS `/Applications/Nemessix.app` exists and real 3DS-family save roots were
  observed, but Nemessix was running during this pass. No restore or upload was
  attempted from that live directory because phase1 policy forbids live
  overwrite and forbids treating a dirty/path observation as a stable snapshot
  boundary.
- A later stopped-state macOS Nemessix pass used
  `scripts/macos-nemessix-stopped-snapshot-e2e.sh`. The script refused live
  Nemessix processes, constrained the root under the Nemessix application
  support directory, rejected symlinks, compared two aggregate fingerprints
  across a 2-second stability window and then built an encrypted snapshot
  candidate without printing filenames, full paths or file contents. Evidence:

```json
{"adapter":"Nemessix 3DS","chunk_count":3,"emulator_stopped":true,"fingerprint":{"file_count":3,"total_bytes":53764,"tree_sha256":"45e36514d87ca30dc6c42db0c3b1a5dc773f24bcbe6ef9694f59f0a3183a4d1d"},"macos_nemessix_stopped_snapshot_e2e":true,"manifest_entries":3,"platform":"macOS","snapshot_file_count":3,"snapshot_id":"e57a7f7b08a45bf31539f2af41570a853d4b9423c551edd89017d17f81710b13","snapshot_total_bytes":53764,"stability_window_seconds":2,"support_level":"Stopped stable snapshot evidence only; not RuntimeVerified until restore is read back by the emulator after relaunch.","title_id":"00048100"}
```

  The encrypted snapshot id is run-specific because snapshot encryption uses random nonces; the stable reproducible evidence is the stopped-process precondition, 2-second matching tree fingerprint, file count and byte count.

  This closes the stopped stable-snapshot proof for macOS Nemessix path
  handling, but still does **not** upgrade macOS Nemessix to `Runtime Verified`
  because no emulator-readable restore/relaunch round trip was performed.

Open Phase 1D gates:

- real macOS Nemessix save-complete IPC and emulator-readable restore/relaunch
  proof; stopped stable snapshot proof exists for the observed local save root;
- Android Nemessix restore proof against a real authorized save root;
- Android Azahar or Citra MMJ modification producing a macOS conflict branch;
- exported `.mhsavebundle` restore in a real emulator-readable no-server
  environment; fixture byte-for-byte recovery is already covered;
- production TLS ingress/reverse proxy for the public Alpha API; the current
  `39082` TCP proxy is an alpha convenience;
- PR CI green on GitHub after each new feature commit. As of 2026-07-07 the
  MHToolkit self-hosted runner is online but limited to one 2c4g host, so the
  workflow intentionally serializes heavy Rust and Android gates and status
  checks must remain low-frequency.
