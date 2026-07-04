# Self-Hosting Benchmark and Failure Injection Ledger

- Status: baseline plan plus current host facts; benchmark values are filled only after Compose artifacts exist and are executed.
- Last updated: 2026-07-04
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
| Local macOS | Docker CLI exists but daemon was not assumed running in earlier probe; Podman 5.8.2 machine was reported available by prior investigation. | 2026-07-04 | Compose tests should support Docker or Podman. Do not claim local cold-start passed until rerun. |
| Aliyun test host | SSH reachable on port `22227` as `ecs-user`; Docker available; Compose `2.33.1`; existing ports 80/443/5432 already used. | 2026-07-04 | Deploy only on isolated project name, private network, and non-conflicting external port. Never touch `nemessix-room`. |
| Repository | `deploy/compose` now exists with PostgreSQL, MinIO, server Dockerfile, migrations, secret-file placeholders and backup/restore/verify scripts. `rtk docker compose -f deploy/compose/compose.yaml config --quiet` passed on 2026-07-04. | 2026-07-04 | Syntax/config gate passed; cold-start/failure benchmark still pending daemon run. |

## 3. Benchmark matrix

| Test | Procedure | Required evidence | Status |
| --- | --- | --- | --- |
| Cold start | `docker compose up -d --wait` from clean volume set. | wall-clock, image digests, health JSON, migration version. | Pending runtime; compose config syntax passed. |
| Upgrade | Run v1, create synthetic account/snapshot, deploy v1+1 migration. | pre/post schema version, head/object verification. | Pending. |
| Rollback | Restore previous server image with compatible schema or documented migration rollback. | runbook transcript and readiness result. | Pending. |
| PostgreSQL backup/restore | Dump database after committed snapshots, destroy DB volume, restore. | `pg_dump` checksum, restored graph verification. | Pending. |
| MinIO backup/restore | Mirror object bucket, destroy object volume, restore. | object count/size, manifest/chunk verification. | Pending. |
| Chunk upload interruption | Kill client/server during multipart upload; resume missing-set. | orphan upload count, resumed commit ID, no bad HEAD. | Pending. |
| Manifest loss | Delete an object referenced before snapshot commit; commit must fail. | HTTP error, audit row, no HEAD update. | Pending. |
| DB commit crash | Crash after object upload before DB insert; GC later reclaims orphan after grace. | orphan list before/after GC. | Pending. |
| HEAD CAS race | Two clients commit on same base. | one fast-forward, one conflict branch; both snapshots retained. | Pending. |
| Resource idle | 10 minute idle service with no changes. | CPU, RSS, object/DB I/O. | Pending. |

## 4. Compose implementation requirements

`deploy/compose` must contain:

- `compose.yaml` with healthchecks, resource limits, named volumes, project-local network, non-root server user and secret-file mounts: present.
- SQL migrations: `migrations/001_init.sql` present.
- Backup/restore/verify/orphan scripts: present; `scripts/migrate.sh` is still pending because PostgreSQL entrypoint applies v1 schema in phase 1.
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

## 6. Benchmark result template

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
