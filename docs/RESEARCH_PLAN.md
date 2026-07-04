# Research Plan

Research claims must cite official documentation, official source, a standard or
a peer-reviewed paper. Every experiment records the date, exact commands,
software/device fingerprint, redacted evidence, confidence and adopt/reject
impact. A successful HTTP response, package installation or fixture alone does
not qualify as runtime compatibility.

## R1: Mature cloud-save systems

Compare lifecycle boundaries, conflict preservation, atomic commit, immutable
history, retention and failure behavior across Steam, Nintendo Switch, PS5,
Google Play Games, Apple iCloud, Syncthing, restic and S3-compatible storage.

Exit: `research/CLOUD_SAVE_SYSTEMS.md` identifies adopted mechanisms and
explicitly rejects silent last-write-wins.

## R2: Emulator adapter evidence

For each platform/emulator capture package or bundle identity, process identity,
user-root acquisition, title/region/slot mapping, includes/excludes, lifecycle
signals, permission model, restore preconditions and an evidence fingerprint.

Exit: all Runtime Verified entries have a real build, real path acquisition and
round-trip save/restore evidence. Fixture-only entries remain Experimental.

## R3: Save-write timelines

Observe manual save, autosave, quest completion, backgrounding, normal exit,
forced exit, simulated power loss, disk full and permission revocation. Record
file events, open/write/fsync/rename behavior, tree fingerprints and emulator
logs without collecting user save contents.

Exit: 1,000 synthetic controlled candidates produce no half-written snapshot;
stability windows and adapter validators are frozen by ADR.

## R4: Cryptography and conflict model

Threat-model server disclosure, malicious storage, revoked/stolen devices,
rollback, corrupt objects, hostile manifests, local IPC spoofing and diagnostic
leaks. Produce deterministic, non-secret test vectors.

Exit: independent vectors verify key separation and AEAD failure behavior; the
server cannot decrypt content or learn plaintext paths; DAG divergence always
preserves both branches.

## R5: Self hosting

Measure clean Compose startup, health, migration, upgrade, rollback, database
and object backup/restore, interrupted upload cleanup, PostgreSQL loss, MinIO
loss and resource use.

Exit: an empty host reaches health through one documented path and a destroyed
deployment is restored from both PostgreSQL and object backups.

## R6: Client/platform feasibility

Spike shared Rust APIs through Swift and Kotlin. Verify macOS Keychain,
FSEvents/process lifecycle/helper constraints and Android Keystore, SAF,
FileObserver, foreground service and WorkManager constraints.

Exit: both native shells invoke one shared snapshot/conflict implementation;
platform background behavior is documented and tested.

## Gate

Implementation begins only after the adapter, trigger, snapshot, crypto,
conflict, storage and platform-policy ADRs are accepted. Any later evidence that
invalidates an assumption reopens the relevant ADR instead of being hidden by a
compatibility workaround.

