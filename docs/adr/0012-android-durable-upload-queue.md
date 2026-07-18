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

Android session-exit, explicit save-complete and manual upload reconciliation
use this ordering:

1. SAF is copied twice into app-private read-only staging and both fingerprints
   must match.
2. Rust creates the immutable E2EE snapshot.
3. The encrypted `.mhsavebundle` is written with temp-file + fsync + rename and
   the object directory is synced.
4. A SQLite WAL transaction records snapshot, server, logical save, exact CAS
   base/parent, device and relative bundle path as `pending`.
5. Plaintext staging is deleted.
6. The worker consumes pending bundles FIFO through the existing signed E2EE
   upload protocol. Failure increments `attempts`, records only a redacted error
   code and returns WorkManager retry; success marks the row `completed` before
   deleting the local encrypted bundle.

Multiple offline captures chain from the latest pending snapshot ID. They do
not use mtime and do not silently replace a different remote HEAD. A stale base
therefore becomes a server conflict branch.

Periodic WorkManager remains at the Android minimum 15-minute class and only
creates a candidate when a durable dirty flag exists and the emulator is
stopped. Wi-Fi/unmetered, battery-not-low and optional charging constraints are
applied. One-time session-exit work is intentionally allowed to run offline so
it can create the local encrypted queue before requesting network retry.

This pipeline uploads only. It never downloads or restores into a live
emulator directory.

## Migration and rollback

Migration is additive. On open, the client adds nullable upload metadata
columns and a unique snapshot index to existing SQLite databases. Legacy rows
without durable bundle metadata are not treated as uploadable Android jobs.

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
