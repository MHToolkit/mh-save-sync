# Phase 1 validation evidence ledger

- Status: live evidence ledger, not a stability claim
- Last updated: 2026-07-05
- Git commit when captured: `0b6d94d` plus uncommitted phase1 changes
- Secret policy: commands below use external secret files under
  `~/Documents/Secrets`; no recovery phrase, access token, device secret,
  plaintext save bytes or user save path content is recorded here.

## Local gates executed

```text
cargo fmt --all -- --check                                      PASS
cargo test --workspace                                          PASS: 19 tests / 14 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
cargo run -p save-cli --bin mh-save -- crypto-device-fixture    PASS: matches tests/fixtures/device-identity-public.json
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

The local Clippy command reports a Rust future-incompatibility notice in
`sqlx-postgres v0.8.0`; it does not emit crate warnings from this workspace.

## Artifact hashes

```text
Android debug APK:
ccfa6f5b9d842cb2c363c4d2338a5a8777039ba8ad16f04d96c92c4ee860a307  apps/android/app/build/outputs/apk/debug/app-debug.apk

Rust client cdylib:
e6bb954a0bc408c9b45b518743a3c29a60512f76cf63c61c95cd781ad2f7d04d  target/release/libsave_client.dylib

Generated UniFFI bindings:
dd579e3f4b47cfbd8e91d326b55be2f72cff3a74ec34faee9227faceec99edc8  artifacts/uniffi/kotlin/uniffi/save_client/save_client.kt
6f9d6af05b44b02cd72d69e22ed9448c1f76732ddf06f2989d3ba0823d2cb9b1  artifacts/uniffi/swift/save_client.swift
eec32706d026d26b8c08eae4d83757d59b0faf4403ed14140988b692fe073885  artifacts/uniffi/swift/save_clientFFI.h
2fb10eea39f366ef73ec22e7d2407dc3167e0f7ee2147281931d3ab64b58a40c  artifacts/uniffi/swift/save_clientFFI.modulemap

Destructive restore backup:
57fadad41befe8b47014ab631afaba023cfa46412d39b2ffac6f3ecf50a13f2a  ~/Games/Backups/MHSaveSync/20260705-025043/postgres.sql
5354669dd7b8e2730d87e3b9a84a5ca7ca3e4c4dab071aee7989cfa572964afa  ~/Games/Backups/MHSaveSync/20260705-025043/minio-data.tar
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
  "history_count": 3
}
```

Restart-resume probe:

```json
{
  "head": "4bbb750a14779f277ffd4d314f03b504ea2a4b03809a0e66d9fa9ec8c1f220da",
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
Server image id:     90b90ed78d9ae8766436cd3c5e55523c5d3125d23b065c262469618113955891
PostgreSQL image id: 5db836939fe3760739047801b3e588e97c8774d02807db98d6e977ec6a5e54a6
MinIO manifest:      sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e
MinIO arm64 image:   8f08aee614800a237906bd48114d733e5ac5bfac4ccdf731f141b0e880d7a253
SQLx migration:      1
Post-restore sample: server 0.13% CPU / 2.044 MiB RSS; PostgreSQL 1.70% / 52.18 MiB; MinIO 1.56% / 73.85 MiB
```

## Runtime support boundary

This evidence proves the shared engine/server/Android shell/self-hosting
fixtures listed above. It does **not** upgrade any path-only emulator entry to
`Runtime Verified`. Runtime Verified still requires reproducible real emulator
save/read evidence in `docs/research/EMULATOR_SAVE_MATRIX.md`.

Open Phase 1D gates:

- add verified `cargo audit` / `cargo deny`, SBOM and artifact-checksum jobs to
  CI without introducing false-positive or unreviewed allowlist drift;
- real macOS Nemessix save-complete IPC and automatic stable snapshot proof;
- Android Nemessix restore proof against a real authorized save root;
- Android Azahar or Citra MMJ modification producing a macOS conflict branch;
- exported `.mhsavebundle` restore in a no-server environment;
- isolated remote deployment and recovery, without touching `nemessix-room`;
- PR CI green on GitHub after the expanded Android/Compose jobs land.
