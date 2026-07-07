# Phase 1 validation evidence ledger

- Status: live evidence ledger, not a stability claim
- Last updated: 2026-07-07
- Git commit when captured: this branch state; exact commit reported in PR/final status
- Secret policy: commands below use external secret files under
  `~/Documents/Secrets`; no recovery phrase, access token, device secret,
  plaintext save bytes or user save path content is recorded here.

## Local gates executed

```text
cargo fmt --all -- --check                                      PASS
cargo test --workspace                                          PASS: 26 tests / 16 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
cargo build --workspace --bins                                  PASS
scripts/supply-chain-gate.sh                                    PASS: cargo-deny + cargo-audit + CycloneDX SBOM
cargo run -p save-cli --bin mh-save -- crypto-device-fixture    PASS: matches tests/fixtures/device-identity-public.json
cargo test -p save-cli --test bundle_cli                       PASS: 2 tests / 1 suite
cargo test -p save-cli --test server_sync_cli                  PASS: 2 tests / 1 suite
scripts/offline-bundle-e2e.sh                                   PASS: export bundle, restore, running fail-closed
scripts/server-sync-e2e.sh                                      PASS: upload/status/restore, conflict branch retained
scripts/macos-shell-e2e.sh                                      PASS: macOS shell upload/status/restore visible
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
running-emulator fail-closed guard. It is still a SwiftPM/menu-bar alpha shell,
not a signed `.app` installer or LaunchAgent deployment.

## UX correction gates executed on 2026-07-07

```text
cargo fmt --all -- --check                                      PASS
git diff --check                                                PASS
cargo test --workspace                                          PASS: 26 tests / 16 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
Android assembleDebug testDebugUnitTest lintDebug               PASS
swift build --package-path apps/macos                           PASS
scripts/build-macos-app-bundle.sh                               PASS: generated local double-clickable .app bundle
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
- macOS SwiftPM smoke keeps CI-friendly CLI mode, adds `--app` menu-bar shell
  with status, pre-launch check, conflict and cloud-unavailable actions, and
  `scripts/build-macos-app-bundle.sh` builds a local
  `artifacts/macos/MH Save Sync.app` with `LSUIElement` menu-bar behavior for
  double-click testing.
- Shared Rust client exposes Chinese launch-gate/conflict decision records for
  future UniFFI UI wiring and tests cloud-unavailable, remote-newer and conflict
  behavior without last-write-wins.
- Final pass also fixes the local `scripts/secret-scan.sh` empty-untracked
  false positive introduced by the runner-migration update, so secret scanning
  remains fail-closed for real matches without failing on an empty local list.

Artifact hashes from this correction:

```text
Android debug APK:
40228506f23b831127efcbeeb520b880a6a3dfd6ff45c9de7faaef88c0114b87  apps/android/app/build/outputs/apk/debug/app-debug.apk

macOS smoke executable:
e1a68fa699680ce637b4bcb8ea1677ed96604337fd941be0bfb53e5a7eb98228  apps/macos/.build/debug/MHSaveSyncMac

macOS local app executable:
e1a68fa699680ce637b4bcb8ea1677ed96604337fd941be0bfb53e5a7eb98228  artifacts/macos/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac
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

## Self-hosted runner throttling check executed on 2026-07-07

Command:

```bash
gh api orgs/MHToolkit/actions/runners --paginate \
  --jq '.runners[] | {name,os,status,busy,labels:[.labels[].name]}'
```

Evidence:

```json
{"busy":false,"labels":["self-hosted","Linux","X64","ecs","ci-general","linux-x64","cn-hangzhou","2c4g","mhtoolkit"],"name":"ecs-cn-hangzhou-mhtoolkit-01","os":"Linux","status":"online"}
```

Adopted CI policy:

- keep workflow-level `cancel-in-progress: true` so stale pushes do not consume
  the self-hosted host;
- serialize heavyweight jobs by making Android depend on Rust, because the
  organization currently has one 2c4g `ci-general` runner;
- avoid high-frequency status watching during development; use single
  `gh pr checks` / `gh run list` snapshots after pushes and wait between checks.

## Artifact hashes

```text
Rust debug binaries:
7345d7b2f1fa0b234816bd89772e8df7688e4724a4f661fc2a6faaeb0d4b2bcf  target/debug/mh-save
3065dd98b545347d3b3446742642299b3703eb3a45789e8116ae9daedd60d3a8  target/debug/mh-save-server

CycloneDX SBOM:
01b91bef41441df28da53f9245442c9d656aca377a394928ae511a1efbc89698  artifacts/sbom/mh-save-sync.cdx.json

Android debug APK:
40228506f23b831127efcbeeb520b880a6a3dfd6ff45c9de7faaef88c0114b87  apps/android/app/build/outputs/apk/debug/app-debug.apk

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

Open Phase 1D gates:

- real macOS Nemessix save-complete IPC and automatic stable snapshot proof;
- Android Nemessix restore proof against a real authorized save root;
- Android Azahar or Citra MMJ modification producing a macOS conflict branch;
- exported `.mhsavebundle` restore in a no-server environment;
- isolated remote deployment and recovery, without touching `nemessix-room`;
- PR CI green on GitHub after each new feature commit. As of 2026-07-07 the
  MHToolkit self-hosted runner is online but limited to one 2c4g host, so the
  workflow intentionally serializes heavy Rust and Android gates and status
  checks must remain low-frequency.
