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

The same transaction also records an account-scoped commit receipt keyed by
upload ID and snapshot ID. It binds the committing device, logical save,
CAS base, manifest, normalized parent set, normalized object-reference set and original
CAS response. A retry after response loss returns that response only when the
opaque content contract matches; a different payload reusing the same snapshot
ID is rejected.

HEAD never references absent objects. A failed CAS records the immutable
snapshot as a conflict branch. Missing-set queries make uploads resumable.
Uncommitted objects are orphan candidates and are reclaimed only after a grace
period and a graph mark pass.

Phase1 GC runs inside the server process rather than exposing storage keys to a
shell pipeline. It defaults to dry-run and takes an explicit `--delete` flag.
Destructive collection persists account-scoped mark/lease rows, then claims and
revalidates one object at a time. S3 deletion holds only a per-account/object
advisory lock shared with begin/upload, never a global hot-table lock. A crash
leaves an expiring lease that can be reclaimed; missing S3 objects are treated
as already swept only when PostgreSQL proves they are not a snapshot or active
upload root.

S3 current-version deletion is not called physical GC when bucket versioning is
enabled. Logical deletion captures the then-current version ID and enqueues that
generation boundary with the opaque storage key in migration 006. The MinIO
Compose wrapper leases that queue, streams keys only through stdin, deletes the
captured version and older generations, and acknowledges only after success.
A newer version uploaded after logical GC is therefore preserved.
Other S3 providers require an equivalent lifecycle/version-purge worker; a
non-zero `physical_purge_pending` is an explicit incomplete state.

The phase1-alpha implementation uses the Rust `object_store` S3 backend with
S3 SHA256 upload checksums enabled. Compose initializes and versions the MinIO
bucket before the API starts. Multipart upload, incomplete-upload lifecycle
cleanup and production bucket policy are still required before a hosted stable
service.

## Backup and rollback

Recovery needs a transactionally consistent PostgreSQL backup plus versioned
object backup. Restore tooling verifies every referenced object before exposing
readiness. Migration `007_commit_receipts.sql` is additive. Rollback may stop
writing/reading receipts only after all pre-rollback client queues have either
received their commit response or been reconciled from snapshot history;
dropping the table earlier reopens the response-loss retry ambiguity but does
not alter immutable snapshots or HEAD rows.
## Phase1-alpha evidence

`save-server` now has both an in-memory test backend and a persistent
PostgreSQL/S3-compatible backend. Local Podman runs on 2026-07-05 proved:

- encrypted fixture objects were checksum-verified by the request body hash and
  stored through `object_store` with S3 SHA256 upload checksums before the
  snapshot transaction;
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
  head=119beee8ef738ddf81cceba508a7ef8801b6e5cc572e9ecf44302bfc43e20fc1
backup: PostgreSQL sha256=7d4b439072fd79fd9ad012dee9b1eba589140b5857381116d66b4c47c6f0f7f3
backup: MinIO tar sha256=b1322d19dcd6eaab71ae8e31b7af77a02ba6fc4db6cd72c6c12929f02bd7163f
restore: readiness 200 and dangling_snapshot_objects=0
crash/GC: before-commit rollback leaves no snapshot or HEAD and the orphan is
  reclaimable; after-commit response loss leaves snapshot/HEAD durable, a
  content-bound commit receipt makes the same upload retry return its original
  CAS outcome, and GC retains the referenced object. Reusing the snapshot ID
  with different logical save, manifest, parents, device or object references
  fails closed; the replay does not poison later commits.
GC Compose: 1,005 untracked keys crossed the S3 listing page boundary; dry-run
  found 1,006 total candidates, delete removed exactly 1,006, PostgreSQL and
  MinIO version listing retained exactly one referenced live version and zero
  orphan noncurrent versions/delete markers; failure stderr exposed no key
```

Multipart upload, quota enforcement, lifecycle cleanup and remote-host
validation remain Phase 1B/1D gates.
