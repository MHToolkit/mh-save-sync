# Phase 1 validation evidence ledger

- Status: live evidence ledger, not a stability claim
- Last updated: 2026-07-05
- Git commit when captured: this branch state; exact commit reported in PR/final status
- Secret policy: commands below use external secret files under
  `~/Documents/Secrets`; no recovery phrase, access token, device secret,
  plaintext save bytes or user save path content is recorded here.

## Local gates executed

```text
cargo fmt --all -- --check                                      PASS
cargo test --workspace                                          PASS: 19 tests / 14 suites
cargo clippy --workspace --all-targets -- -D warnings           PASS
cargo build --workspace --bins                                  PASS
scripts/supply-chain-gate.sh                                    PASS: cargo-deny + cargo-audit + CycloneDX SBOM
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

The 2026-07-05 supply-chain pass upgraded the server stack away from
`sqlx 0.8.0` and the old AWS SDK TLS chain. `cargo-deny` and `cargo-audit`
now pass with two reviewed temporary ignores for `quick-xml` advisories
`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`, both transitive through
`object_store 0.14` S3 response XML parsing. The server does not parse
user-provided XML; the ignore must be removed when object_store releases a
quick-xml `>=0.41` update.

## Artifact hashes

```text
Rust debug binaries:
aaef1fb5aa8e9159a8ed85cbdfa34763cab8d79545e39e5644616ccf1d57b259  target/debug/mh-save
21f8577f8e8c04738ee86cb99116d2f89d2c3b3d5a22e7c9a7b824a67bc418e0  target/debug/mh-save-server

CycloneDX SBOM:
9a0630cd92f510b4c39e232ceb1bf5ccdfc19e595e7495268323c612b3aa2818  artifacts/sbom/mh-save-sync.cdx.json

Android debug APK:
ccfa6f5b9d842cb2c363c4d2338a5a8777039ba8ad16f04d96c92c4ee860a307  apps/android/app/build/outputs/apk/debug/app-debug.apk

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
