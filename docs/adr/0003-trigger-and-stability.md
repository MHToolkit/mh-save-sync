# ADR 0003: Session boundaries, dirty hints and stable staging

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

Trigger priority is fixed:

1. authenticated emulator `save-complete`;
2. FSEvents/FileObserver/SAF reconciliation marks a logical save dirty;
3. normal game/emulator exit forces reconciliation;
4. macOS helper and Android WorkManager perform periodic reconciliation;
5. explicit sync/upload/download.

A watcher never uploads. A candidate must pass debounce, two consecutive stable
tree fingerprints, read-only staging copy, manifest/hash generation and the
adapter consistency validator. Staging is fingerprinted again; mismatch retries
without creating a snapshot.

The initial default is a 2-second debounce and two equal observations 500 ms
apart, bounded by 10 seconds. These are adapter-overridable only with timeline
evidence. Process exit bypasses debounce but not stability/consistency.

While an emulator runs, only a validated stable upload is allowed. Downloads
land in local CAS; restore is blocked. A toolkit-managed launch performs a
pre-launch remote check. A launch outside the toolkit only surfaces status.

Android periodic work assumes no interval below 15 minutes. Active sessions use
a foreground service; exit schedules one-time work. macOS uses process events
and FSEvents rather than high-frequency whole-disk polling.

## Failure behavior

Timeout, permission loss, disk pressure or validator failure leaves the original
save untouched and retains a dirty/pending state with an actionable error.
## Phase1-alpha evidence

`rtk cargo test --workspace` includes a 1,000-iteration synthetic dirty-candidate loop, watcher-direct-upload refusal, running-emulator restore refusal and fixture restore tests. Real emulator timeline evidence is still tracked separately.
