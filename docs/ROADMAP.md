# Roadmap and Gates

No channel is called stable until every data-integrity gate passes.

## Phase 0: Research and contract freeze

- Complete R1-R6 and record raw evidence.
- Freeze GameKey, AdapterDescriptor, snapshot/manifest format, key hierarchy,
  DAG rules, server commit ordering and background policies.
- Establish synthetic fixtures and cross-language vectors.

Gate: all material ADRs accepted; unsupported/runtime-unverified combinations
are explicitly labeled; no open P0 security or data-loss assumption.

## Phase 1A: Local engine

- SQLite WAL metadata and encrypted local CAS.
- Stable candidate staging, fixed 1 MiB chunks, immutable snapshot DAG.
- Safe restore, export/import, retention and mark/sweep GC.
- Generic folder and evidence-backed emulator adapters.

Gate: 1,000-candidate test, hostile manifest suite and interrupted restore
rollback pass.

## Phase 1B: Protocol and service

- Account/device registration and revocation.
- Resumable missing-chunk upload.
- Durable chunks, then manifest, then snapshot row, then CAS HEAD.
- PostgreSQL/S3 production mode and SQLite/filesystem development mode.

Gate: failure injection never exposes a HEAD whose objects are absent; two- and
three-device divergence preserves every branch.

## Phase 1C: Native clients

- macOS menu-bar app/helper with FSEvents and process lifecycle.
- Android Compose app with SAF, foreground session service and WorkManager.
- Shared onboarding, device, status, manual action, conflict, history, export
  and retention surfaces.

Gate: macOS and Android use the shared engine and pass real-device round trips.

## Phase 1D: Self-hosting and alpha

- Compose healthchecks, migrations, secret files, resource limits, backup,
  restore, upgrade and rollback.
- CI data-integrity E2E, SBOM, checksums and secret scanning.
- Optional isolated remote test deployment.

Gate: real macOS ↔ Android ↔ second-emulator flow, offline conflict, damaged
local restore, destroyed-service restore and serverless bundle recovery pass.

## Release channels

`nightly` may change formats. `alpha` requires migrations and rollback.
`beta` requires feature freeze and a published compatibility matrix. `stable`
requires zero open P0 security/data-loss defects and all runtime claims backed
by current evidence.

