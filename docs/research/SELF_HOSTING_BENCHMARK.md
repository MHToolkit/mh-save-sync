# Self-Hosting Benchmark and Failure Injection Ledger

- Status: local Podman cold-start, persistence, conflict, restart-resume, backup, destructive restore and >2,000-object readiness scan verified; remote hardening and long-idle runs remain open.
- Last updated: 2026-07-18
- Scope: Docker/Podman local self-hosting and optional isolated Aliyun deployment on `8.130.112.207` without touching existing `nemessix-room` services.

## 1. Deployment shape to benchmark

Production-compatible self-hosting uses:

- `save-server`: Rust/Axum API, non-root container, readiness endpoint checks database and object store.
- PostgreSQL: truth source for accounts, devices, profiles, logical saves, snapshot graph, upload sessions, quotas, retention and audit.
- MinIO or S3-compatible object store: encrypted chunks, manifests and exports only.
- No Redis in phase 1. Redis is not a save truth source.
- Secret files under `deploy/compose/secrets/` for local demos; real machine secrets under `~/Documents/Secrets/mh-save-sync.env` with mode `0600` and quoted values.

Commit ordering benchmark invariant:

```text
encrypted chunks durable + checksum verified
  -> encrypted manifest durable + checksum verified
  -> PostgreSQL immutable snapshot row in transaction
  -> compare-and-swap logical HEAD
```

Readiness must fail if any committed HEAD references a missing manifest/chunk.

## 2. Current host facts

| Environment | Fact | Evidence date | Decision |
| --- | --- | --- | --- |
| Local macOS | Docker daemon was unavailable. Podman 5.7.1 machine (AppleHV, 5 CPUs, 4 GiB RAM) successfully built and ran the stack. | 2026-07-05 | Scripts accept `CONTAINER_RUNTIME=podman`; Docker remains the documented production default. |
| Aliyun test host | SSH reachable on port `22227` as `ecs-user`; Docker available; Compose `2.33.1`; existing ports 80/443/5432 already used. | 2026-07-04 | Deploy only on isolated project name, private network, and non-conflicting external port. Never touch `nemessix-room`. |
| Repository | `deploy/compose` includes PostgreSQL, pinned MinIO, a one-shot bucket/versioning initializer, non-root server image, SQLx migrations, external secret files and backup/restore/verify scripts. Compose runtime, black-box E2E, restart-resume and destructive restore passed locally after migrating the server to `object_store` S3 with SHA256 upload checksums. | 2026-07-05 | Local self-hosting baseline is reproducible; PR CI now gates compose E2E, while remote isolation remains a separate gate. |

## 3. Benchmark matrix

| Test | Procedure | Required evidence | Status |
| --- | --- | --- | --- |
| Cold start | `podman compose ... up -d --build --wait` from clean volumes. | wall-clock, image digests, health JSON, migration version. | **Passed locally.** Source rebuild after object-store hardening: ~4m; service start/health after build: ~10 s; build context: 163.5 KiB. `/ready` returned `postgres-s3`; SQLx migration version `1`. |
| Upgrade | Run v1, create synthetic account/snapshot, deploy v1+1 migration. | pre/post schema version, head/object verification. | Pending. |
| Rollback | Restore previous server image with compatible schema or documented migration rollback. | runbook transcript and readiness result. | Pending. |
| PostgreSQL backup/restore | Stop API writers, dump database, destroy both volumes, restore. | `pg_dump` checksum, restored graph verification. | **Passed locally.** PostgreSQL SHA-256 `7d4b439072fd79fd9ad012dee9b1eba589140b5857381116d66b4c47c6f0f7f3`; restored graph passed readiness and object verification. |
| MinIO backup/restore | Stop API writers, archive object volume, destroy both volumes, restore. | archive checksum, referenced-object verification. | **Passed locally.** archive SHA-256 `b1322d19dcd6eaab71ae8e31b7af77a02ba6fc4db6cd72c6c12929f02bd7163f`; `dangling_snapshot_objects=0` after destructive restore. |
| Chunk upload interruption | Upload a chunk, stop/restart server, upload manifest and commit same upload session. | resumed commit ID, no bad HEAD. | **Passed locally.** upload session survived restart and committed as first snapshot; HEAD `119beee8ef738ddf81cceba508a7ef8801b6e5cc572e9ecf44302bfc43e20fc1`. |
| Committed object loss | Delete a MinIO object referenced by committed history. | readiness failure, then successful disaster restore. | **Passed locally.** `/ready` returned HTTP 503 with `missing-object`; destructive two-store restore returned readiness to 200 with zero dangling references. |
| Readiness beyond 2,000 objects | One committed snapshot references 2,001 object rows; omit only the final object, then add it and retry. | first `/ready` is 503 without leaking the storage key; second is ready; no total scan cap. | **Passed locally.** Persistent readiness now uses a repeatable-read transaction and 256-row keyset pages. `scripts/readiness-fullscan-test.sh` runs the real PostgreSQL fixture; the former fixed `LIMIT 2000` regression is covered. |
| DB commit crash | Crash after object upload before DB insert; GC later reclaims orphan after grace. | orphan list before/after GC. | Pending. |
| HEAD CAS race | Two upload sessions commit on the same base. | one fast-forward, one conflict branch; both snapshots retained. | **Passed locally.** Black-box API run produced three retained snapshots, one conflict branch and unchanged HEAD after stale-base commit. |
| Resource idle | 10 minute idle service with no changes. | CPU, RSS, object/DB I/O. | Partial. One post-restore sample: server 0.13% CPU / 2.044 MiB RSS; PostgreSQL 1.70% / 52.18 MiB; MinIO 1.56% / 73.85 MiB. Ten-minute series remains pending. |

## 4. Compose implementation requirements

`deploy/compose` contains:

- `compose.yaml` with healthchecks, resource limits, named volumes, project-local network, non-root server user and secret-file mounts: present.
- SQL migrations: `migrations/001_init.sql` present.
- Backup/restore/verify/orphan scripts: present. The server applies embedded SQLx migrations before accepting traffic.
- `README.md` with 5-minute local demo, upgrade/rollback and disaster-recovery runbook.
- `.env.example` with quoted non-secret placeholders. Real `.env` is ignored and stored outside the repo.

## 5. Failure injection acceptance criteria

A failure injection passes only if:

1. local emulator directories are untouched;
2. SQLite/local CAS retains queued snapshots;
3. server readiness does not expose an inconsistent committed HEAD;
4. every committed snapshot references durable encrypted objects;
5. orphan chunks/manifests are not reachable from user history and are removed only after grace;
6. restoring PostgreSQL without matching objects is detected as not ready;
7. restoring objects without matching PostgreSQL does not invent history;
8. logs contain no recovery phrase, token, plaintext path content or save bytes.

## 6. Local run record

```text
Run ID: local-podman-20260705
Host: Apple Silicon macOS, Podman machine 5 CPUs / 4 GiB
Server image id: 31d1fd65cfab353434525dbc873f32b28b779e266bc76aa41f7de828bc8b51e4
PostgreSQL image id: 5db836939fe3760739047801b3e588e97c8774d02807db98d6e977ec6a5e54a6
MinIO manifest digest: sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e
MinIO local arm64 image id: 8f08aee614800a237906bd48114d733e5ac5bfac4ccdf731f141b0e880d7a253
Readiness: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
Synthetic API result: 3 snapshots, 1 stale-base conflict, chunk missing-set dedupe passed, corrupt checksum rejected, signed certificate validation fail-closed
Restart injection: chunk-before-restart + manifest/commit-after-restart passed; resumed HEAD 119beee8ef738ddf81cceba508a7ef8801b6e5cc572e9ecf44302bfc43e20fc1
Destructive recovery: both named volumes removed, PostgreSQL and MinIO restored, no dangling references
Backup artifact hashes: postgres.sql sha256=7d4b439072fd79fd9ad012dee9b1eba589140b5857381116d66b4c47c6f0f7f3; minio-data.tar sha256=b1322d19dcd6eaab71ae8e31b7af77a02ba6fc4db6cd72c6c12929f02bd7163f
```

Raw synthetic bytes are generated by `scripts/compose-e2e.py`; no user save,
credential, recovery secret or plaintext path content is part of the fixture.
The backup artifacts are outside the repository under
`~/Games/Backups/MHSaveSync/`.

## 7. Remaining benchmark template

```text
Run ID:
Date:
Host:
Git commit:
Images:
Command:
Cold start seconds:
Readiness JSON:
Synthetic snapshots committed:
Conflict cases:
Backup artifacts + SHA256:
Restore verification:
CPU/RSS idle:
Failures injected:
Adopt/reject impact:
```
