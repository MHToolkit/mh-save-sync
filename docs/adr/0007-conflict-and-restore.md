# ADR 0007: Parent DAG conflicts and offline safe restore

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

Each snapshot carries one or more parent snapshot IDs. Same-ancestor updates
fast-forward; neither-ancestor updates create conflict branches. Wall-clock time
and mtime are display evidence only and never decide correctness. Delete is a
tombstone and conflicts with a concurrent modification.

The UI compares device, recorded time, parents, size and hashes without claiming
semantic merging of binary saves. A user may select one branch as the next HEAD
or copy it to a new logical slot; neither action deletes the other history.

Phase 1 also defines a game-specific diff parser contract. Diff parsing runs on
the client after decrypting manifests or local folders, never on the server. The
initial `mh3g-3ds` parser reports conservative file/byte-level differences
(changed files, sizes, hashes and byte ranges) and explicitly does not claim to
understand hunter names, equipment, items or quest progress. Semantic fields are
allowed only after a game-specific parser has fixtures, evidence, fail-closed
rules and an ADR; no parser may silently convert between 3G/3U, 4G/4U or XX/GU.

Restore sequence:

1. decrypt and verify the chosen snapshot into isolated staging;
2. validate manifest limits and adapter consistency;
3. snapshot the current target;
4. acquire the logical-save lease and prove the emulator stopped;
5. commit by atomic same-filesystem replacement;
6. reopen, fingerprint and verify; rollback on mismatch.

For Android SAF without atomic directory exchange, use a durable journal,
per-file staged replacement and reverse-order rollback. Interrupted journals
are recovered before any new restore.
## Phase1-alpha evidence

Unit tests preserve stale-base commits as conflict branches and block restore while emulator state is Running. Folder restore backs up the current directory before replacement and rolls back on reconstruction failure.
