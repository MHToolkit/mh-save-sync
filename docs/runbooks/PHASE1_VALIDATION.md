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
cargo test --workspace                                          PASS: 24 tests / 15 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
cargo build --workspace --bins                                  PASS
scripts/supply-chain-gate.sh                                    PASS: cargo-deny + cargo-audit + CycloneDX SBOM
cargo run -p save-cli --bin mh-save -- crypto-device-fixture    PASS: matches tests/fixtures/device-identity-public.json
cargo test -p save-cli --test bundle_cli                       PASS: 2 tests / 1 suite
scripts/offline-bundle-e2e.sh                                   PASS: export bundle, restore, running fail-closed
cargo build --release -p save-client                            PASS
UniFFI Kotlin binding generation                                PASS
UniFFI Swift binding generation                                 PASS
Android assembleDebug lintDebug                                 PASS
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

## UX correction gates executed on 2026-07-07

```text
cargo fmt --all -- --check                                      PASS
git diff --check                                                PASS
cargo test --workspace                                          PASS: 22 tests / 14 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
Android assembleDebug lintDebug                                 PASS
swift build --package-path apps/macos                           PASS
swift run --package-path apps/macos MHSaveSyncMac --status      PASS
swift run --package-path apps/macos MHSaveSyncMac --prelaunch-check PASS
swift run --package-path apps/macos MHSaveSyncMac --conflict-demo PASS
scripts/secret-scan.sh                                          PASS
scripts/artifact-checksums.sh Android APK + macOS executable    PASS
```

UX correction scope:

- Android app label and workbench are Chinese-first for phase1 alpha.
- Android now shows the server destination, `MH3G / Android Nemessix` target,
  per-game enable switch, SAF authorization, pre-launch gate, explicit conflict
  choices, manual upload, download-to-cache-only, active Nemessix session state,
  and visible background reconcile summaries.
- Android foreground notification now states that running sessions forbid cloud
  overwrite and reconcile only after exit.
- macOS SwiftPM smoke keeps CI-friendly CLI mode and adds `--app` menu-bar shell
  with status, pre-launch check, conflict and cloud-unavailable actions.
- Shared Rust client exposes Chinese launch-gate/conflict decision records for
  future UniFFI UI wiring and tests cloud-unavailable, remote-newer and conflict
  behavior without last-write-wins.
- Final pass also fixes the local `scripts/secret-scan.sh` empty-untracked
  false positive introduced by the runner-migration update, so secret scanning
  remains fail-closed for real matches without failing on an empty local list.

Artifact hashes from this correction:

```text
Android debug APK:
8b3f6783284b95ea2708d041c03b206f66b79387169c69b4a3dd919e7905f906  apps/android/app/build/outputs/apk/debug/app-debug.apk

macOS smoke executable:
940b2a61329b78b97b8d15cec1b83d6a47fa1a20a2f55ac195156f82a3faac1a  apps/macos/.build/debug/MHSaveSyncMac
```

## Artifact hashes

```text
Rust debug binaries:
aaef1fb5aa8e9159a8ed85cbdfa34763cab8d79545e39e5644616ccf1d57b259  target/debug/mh-save
21f8577f8e8c04738ee86cb99116d2f89d2c3b3d5a22e7c9a7b824a67bc418e0  target/debug/mh-save-server

CycloneDX SBOM:
9a0630cd92f510b4c39e232ceb1bf5ccdfc19e595e7495268323c612b3aa2818  artifacts/sbom/mh-save-sync.cdx.json

Android debug APK:
8b3f6783284b95ea2708d041c03b206f66b79387169c69b4a3dd919e7905f906  apps/android/app/build/outputs/apk/debug/app-debug.apk

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
- PR CI green on GitHub after the expanded supply-chain, Android and Compose
  jobs can start; the current GitHub account billing/spending-limit failure
  prevents runner execution.
