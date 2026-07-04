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

`save-server` now has both an in-memory test backend and a persistent
PostgreSQL/S3-compatible backend. Local Podman runs on 2026-07-05 proved:

- encrypted fixture objects were checksum-verified and stored in MinIO before
  the snapshot transaction;
- the SQL transaction inserted snapshot, parent and object-reference rows
  before conditionally advancing HEAD;
- two sessions with the same base produced one fast-forward and one retained
  conflict branch;
- an upload resumed after the server container restarted;
- destructive PostgreSQL plus MinIO volume restore recovered the prior HEAD,
  and repository verification reported zero dangling object references.

Latest black-box evidence:

```text
ready: {"status":"ready","version":"0.1.0","backend":"postgres-s3"}
compose-e2e: account_root_immutable=true, certificate_fail_closed=true,
  checksum_fail_closed=true, history_count=3, conflict_count=1,
  dedupe_missing_count=1
restart-resume: resumed_after_restart=true,
  head=4bbb750a14779f277ffd4d314f03b504ea2a4b03809a0e66d9fa9ec8c1f220da
backup: PostgreSQL sha256=57fadad41befe8b47014ab631afaba023cfa46412d39b2ffac6f3ecf50a13f2a
backup: MinIO tar sha256=5354669dd7b8e2730d87e3b9a84a5ca7ca3e4c4dab071aee7989cfa572964afa
restore: readiness 200 and dangling_snapshot_objects=0
```

Multipart upload, signed request authentication, quota enforcement, lifecycle
cleanup and remote-host validation remain Phase 1B gates.
