# ADR 0012: Android durable encrypted upload queue

- Status: Accepted for phase1-alpha
- Date: 2026-07-18
- Owners: MHToolkit maintainers

## Context

The original Android shell scheduled WorkManager jobs but intentionally stopped
before SAF capture and upload. A network failure after emulator exit therefore
had no durable encrypted candidate to resume. The existing SQLite
`upload_queue` table only proved WAL creation and pending counts; it could not
identify or consume an upload.

## Decision

Android confirmed session-exit and manual upload reconciliation use this
ordering. Save-complete is deliberately disabled until a released, pinned
Nemessix event contract exists; `SaveQuiescenceV1` is a restore/copy lease and
is not represented as a save-complete event.

1. SAF is copied twice into app-private read-only staging and both fingerprints
   must match.
2. Rust creates the immutable E2EE snapshot.
3. The encrypted `.mhsavebundle` is written with temp-file + fsync + rename and
   the object directory is synced.
4. A SQLite WAL transaction records snapshot, server, logical save, exact CAS
   base/parent, device, SAF tree binding, stable local fingerprint and relative
   bundle path as `pending`.
5. Plaintext staging is deleted.
6. A capture-only Worker finishes successfully while offline, then schedules a
   separate drain Worker carrying unmetered/connected, battery-not-low and
   optional charging constraints.
7. The drain atomically claims one row with a random 128-bit UUID owner and expiring
   lease. Completion/failure updates require the same owner; an expired lease
   is reclaimable after a crash. It then uses the existing signed E2EE upload
   protocol. Failure increments `attempts` and records only a redacted code;
   success marks the row `completed` and upserts the owner-scoped consistency
   baseline in the same SQLite transaction before deleting the encrypted
   bundle. A non-conflict response whose HEAD is not the uploaded snapshot
   fails closed and remains retryable.

The next capture chooses its base in this order: latest pending local snapshot,
durable SQLite consistency HEAD for the exact endpoint/logical-save/tree/device
binding, then the legacy UI ledger only when no durable baseline exists. Thus a
process kill after the server commit but before Kotlin updates the UI cannot
reuse an older HEAD and manufacture a false conflict.

Multiple offline captures chain from the latest pending snapshot ID. They do
not use mtime and do not silently replace a different remote HEAD. A stale base
therefore becomes a server conflict branch.

Periodic WorkManager remains at the Android minimum 15-minute class. Dirty and
captured generations plus the capture lease live in SQLite; claim rereads the
generation atomically, and queue insertion plus acknowledgement use one
transaction. One-time session-exit work
is intentionally allowed to run offline. Every automatic network drain applies
Wi-Fi/unmetered (or user-selected connected), battery-not-low and optional
charging constraints.

When this app launches Nemessix, a foreground service records the session.
Direct emulator launches outside the tool have
no reliable public Android lifecycle callback and therefore remain periodic/
manual reconciliation only. Cross-package `ActivityManager` visibility remains
unverified on the target Android build, so the capability gate is false and the
ActivityManager automatic-exit path is disabled. The explicit UI action only
authorizes dual-stable capture; the independent process gate still fails closed.
An empty queue message therefore never claims that an emulator exit was
observed. A capture-time SAF `SecurityException` removes only the persisted URI
binding and requests reauthorization; encrypted queue objects remain intact.

Queued rows retain the normalized endpoint present when they were created.
Changing settings never hides or silently migrates old rows: a drain consumes
endpoints round-robin, skips a failed endpoint for that run, and sends each row
only to its original endpoint. The UI reports
when pending work spans multiple endpoints.

This pipeline uploads only. It never downloads or restores into a live
emulator directory.

## Migration and rollback

Migration is additive. On open, the client adds nullable upload metadata,
`lease_owner`, `lease_expires_at`, `tree_uri`, `local_fingerprint`, the
`sync_consistency` table and a unique snapshot index to existing SQLite
databases. The former alpha SharedPreferences pending receipt is accepted only
as a one-time migration source: before drain, JNI attaches it to the matching
pending SQLite row using snapshot/endpoint/logical-save/device predicates, and
the preference is cleared only after that durable update succeeds. New queue
items never use SharedPreferences as the receipt truth. Legacy rows without
durable bundle metadata are not treated as uploadable Android jobs. The old `ReconcileWorker` class remains as a
capture-only compatibility target so persisted WorkManager rows do not resolve
to a missing class after upgrade. The alpha.3 boolean dirty bit migrates to one
dirty generation, and its unproven default `session_active=true` is reset once
when the process-evidence tracker version is installed.
Both legacy unique WorkManager names are cancelled once; a Robolectric upgrade
test verifies their persisted rows reach a finished state.
If releasing a failed capture lease cannot be confirmed, WorkManager retries
immediately and reports unknown local queue state rather than silently waiting
for lease expiry.

Rollback may leave encrypted bundles and pending rows in app-private storage;
older clients ignore them and local emulator saves remain untouched. Reinstall
or app-data deletion removes this queue, so users must be told to let pending
uploads finish or export a bundle before destructive app-data operations.

## Consequences

- Network/server outages no longer discard a stable exit snapshot.
- Recovery secrets remain Keystore-wrapped and are loaded only while creating
  or consuming a queue item; they are never written to SQLite or bundles.
- A crash after bundle rename but before the SQLite transaction can leave an
  unreachable encrypted orphan. It cannot affect HEAD and can be reclaimed by
  a future queue-object sweep after a grace period.
- Runtime Verified still requires real-device offline/exit/retry evidence; JVM
  and Rust tests establish deterministic contract coverage only.
