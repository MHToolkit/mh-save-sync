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


## Office Mac ↔ home Android user flow

Phase 1 alpha now uses a Chinese-first sync workbench so the user can see the
actual target instead of pressing an opaque “sync” button:

1. Run or self-host the server and use the same URL on both devices.
   - macOS: set `MH_SAVE_SYNC_SERVER_URL`.
   - Android: enter the server address in the app.
2. macOS Nemessix before launch: run
   `swift run --package-path apps/macos MHSaveSyncMac --prelaunch-check` or
   start the menu-bar shell with `--app`. The gate explains remote-newer,
   conflict and cloud-unavailable choices before the game starts.
3. Android Nemessix before launch: authorize the Nemessix SAF save directory,
   keep `MH3G / Android Nemessix` enabled, then tap `启动前检查`. Android also
   shows `恢复云端到本地（需停止 Nemessix）`; if the session is active it fails closed
   with a visible “没有覆盖本地存档” message.
4. Conflicts list local vs cloud device/time/parent/size/hash and require an
   explicit choice: `云端覆盖本地` or `本地替换云端`. Both histories are retained;
   there is no mtime-based last-write-wins.
5. If the server is unavailable, continue local play is explicit. Local queues
   remain intact and upload resumes after the server recovers.

Detailed Chinese UX flows are maintained in `docs/ux/SYNC_USER_FLOWS.md`.

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
  same host; macOS and Compose evidence remains recorded in
  `docs/runbooks/PHASE1_VALIDATION.md`.

Still not stable: real macOS↔Android↔second-emulator round trips, polished
export/import UX, upgrade/rollback benchmark, remote isolated deployment and
real-emulator bundle recovery remain open gates in `docs/ROADMAP.md`. Fixture
no-server bundle recovery is covered by `scripts/offline-bundle-e2e.sh`.

## Five-minute local demo

```bash
cargo test --workspace
cargo run -p save-cli --bin mh-save -- adapters
cargo run -p save-cli --bin mh-save -- crypto-vector
cargo run -p save-cli --bin mh-save -- crypto-device-fixture
cargo run -p save-cli --bin mh-save -- snapshot-fixture tests/fixtures/generic-save
./scripts/offline-bundle-e2e.sh
./scripts/supply-chain-gate.sh
swift run --package-path apps/macos MHSaveSyncMac
./scripts/macos-shell-e2e.sh
```

macOS shell can call the same Rust CLI pipeline used by Android/CLI demos:

```bash
export MH_SAVE_SYNC_SERVER_URL=http://127.0.0.1:18080
export MH_SAVE_SYNC_CLI="$PWD/target/debug/mh-save"

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
`conflict_snapshot`, so a manual sync is never a black box. The reproducible
gate is `./scripts/server-sync-e2e.sh`: it uploads an office snapshot, uploads
a home/Android-style divergent branch without a base head, and verifies the
cloud HEAD is preserved while the conflict branch is retained.

Android local build/lint, using Android Studio's bundled JBR when macOS has no
system Java:

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug
```


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
```

Backup and destructive restore:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/backup.sh

CONTAINER_RUNTIME=podman \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/restore.sh "$HOME/Games/Backups/MHSaveSync/<run-id>"
```

Latest local validation evidence is summarized in
`docs/runbooks/PHASE1_VALIDATION.md`.
