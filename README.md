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
- macOS Swift shell smoke, Android SAF/WorkManager shell skeleton and Compose self-hosting skeleton.

Still not stable: real macOS↔Android emulator round trips, production PostgreSQL/S3 persistence, export/import UX and remote disaster-recovery benchmark remain open gates in `docs/ROADMAP.md`.

## Five-minute local demo

```bash
cargo test --workspace
cargo run -p save-cli --bin mh-save -- adapters
cargo run -p save-cli --bin mh-save -- crypto-vector
cargo run -p save-cli --bin mh-save -- snapshot-fixture tests/fixtures/generic-save
swift run --package-path apps/macos MHSaveSyncMac
```

Self-hosted config syntax check:

```bash
cd deploy/compose
printf %s local-postgres-password > secrets/postgres_password.txt
printf %s minioadmin > secrets/minio_root_user.txt
printf %s local-minio-password > secrets/minio_root_password.txt
docker compose config --quiet
```

