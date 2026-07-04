# ADR 0006: PostgreSQL graph truth and S3-compatible encrypted objects

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

PostgreSQL is the production truth for accounts, devices, profiles, logical
saves, snapshot graph, upload sessions, quotas, retention and audit. An
S3-compatible store contains encrypted chunks, manifests and exports. Redis is
not required and is never a save truth source.

Commit ordering is:

1. upload and checksum every missing encrypted chunk;
2. upload and checksum the encrypted manifest;
3. in a PostgreSQL transaction, verify the upload session and insert immutable
   snapshot/parent rows;
4. compare-and-swap the logical-save HEAD from `base_head`.

HEAD never references absent objects. A failed CAS records the immutable
snapshot as a conflict branch. Missing-set queries make uploads resumable.
Uncommitted objects are orphan candidates and are reclaimed only after a grace
period and a graph mark pass.

S3 multipart uploads use checksums and lifecycle cleanup for incomplete parts.
A filesystem + SQLite development backend may implement the same protocol and
ordering, but cannot be presented as production-equivalent durability.

## Backup and rollback

Recovery needs a transactionally consistent PostgreSQL backup plus versioned
object backup. Restore tooling verifies every referenced object before exposing
readiness.
## Phase1-alpha evidence

`save-server` exposes health/readiness and in-memory begin/chunk/manifest/commit routes that enforce manifest-before-head and stale-base conflict preservation. `deploy/compose` supplies PostgreSQL/MinIO schema and healthcheck skeleton; production SQLx/S3 persistence remains a Phase 1B gate.
