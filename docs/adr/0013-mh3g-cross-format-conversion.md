# ADR 0013: Japanese MH3G 3DS-to-Cemu conversion

- Status: Accepted for phase1-alpha
- Date: 2026-07-19
- Owners: MHToolkit maintainers
- Review date: 2026-10-19

## Context

The Japanese releases of Monster Hunter 3G on 3DS and Monster Hunter 3G HD Ver.
on Wii U store the same save-body length in different containers and byte
orders. A user needs an offline migration from a 3DS slot created by
Nemessix/Azahar to the corresponding Japanese Cemu slot without modifying the
source save.

The accepted source profile is a `0x8A00`-byte 3DS slot with header
`2B 00 00 00`. The accepted target profile is a `0x8A24`-byte Cemu slot with a
40-byte wrapper whose final byte is `0x2B`; both carry a `0x89FC`-byte save
body. Local Japanese 3DS and Cemu container samples established the `0x2B`
value. This differs from the `0x2C` European/US target wrapper used by the
upstream reference converter.

`fadillzzz/3usavetools` release `0.3.1` is the provenance for the known
endianness intervals, monster-discovery flags, and arena-record transforms.
The pinned source is upstream commit
`d20fea5d98d5c465841c8e5626dae6709622354a`; its `save_indices.py` SHA-256 is
`0753baafad37147cb4701b7315a9deb9055ff699f444d55fce537b4e1ae35deb`.

The official Japanese transfer application, `CTR-N-JMUJ` (Program ID
`00040000000C3400`), is additional format evidence. Static examination found
the MH3G title ID `00048100` and save-archive related strings. Its ARM code has
not been fully reversed, so it is not a source of unverified field mappings.

## Decision

Ship a separate local-only Rust CLI, `mh3g-save-convert`, with exactly one
supported direction:

```text
Japanese MH3G 3DS/Nemessix/Azahar user1|user2|user3
  -> Japanese MH3G HD Cemu user1|user2|user3
```

The first version rejects other regions, games, slot names, reverse conversion,
and automatic conversion by sync/watch services. It reads the source as a
regular local file, performs no network or cloud processing, and never writes
the source file.

The converter constructs the Japanese `0x2B` Cemu wrapper and applies only the
pinned, known transforms. Bytes outside those explicitly listed transformation
ranges are preserved byte-for-byte. Preserved bytes are not a claim of semantic
understanding or full semantic verification.

The official transfer application's archive evidence supports the scope but does
not extend the transformation table. The table remains pinned to the reviewed
`3usavetools 0.3.1` provenance and explicit Japanese-wrapper substitution.

This ADR does not grant Runtime Verified status to Cemu, Nemessix, or Azahar.
Passing unit, differential, or file-level checks is not an emulator load proof.
That status requires a later stopped-emulator install, real Japanese MH3G HD
Cemu load/readback, source-hash verification, and rollback evidence.

## Migration and rollback

`inspect` is read-only. `convert` is dry-run unless `--write` is specified.
Before a write or rollback, Nemessix, Azahar, and Cemu must all be stopped; the
tool also refuses operation when it detects one of these processes.

For a write, the target must designate a Cemu `user1`, `user2`, or `user3`
path in an existing directory. The installer validates the generated target,
writes a same-directory temporary file, atomically replaces the target, and
writes a controlled JSON manifest. If an old target existed, the installer first
creates a same-directory, hash-bound backup. The manifest stores paths and
SHA-256 values, not player content. A failed installation restores the prior
target and removes new transaction artifacts.

`rollback --manifest <path>` requires the controlled slot-bound manifest path
and validates its structure and internal consistency. It restores the backed-up
target when there was one, or removes the newly-created target when the slot was
originally absent; it then removes the transaction artifacts. It refuses a
running emulator, a malformed or inconsistent manifest, a slot/path binding
mismatch, an unexpected installed-target hash, or a backup-content hash
mismatch.

The JSON manifest is not signed or protected by a MAC. It therefore cannot
detect an attacker rewriting the manifest and related files into a new,
self-consistent set. The manifest, target, and backup remain inside the local
filesystem trust boundary.

## Consequences

- Users receive a repeatable Japanese-profile migration with an explicit
  dry-run, write, backup, and rollback lifecycle.
- Unknown save fields remain intact at the byte level but remain semantically
  unverified.
- Operators must retain the manifest until successful Cemu validation or a
  completed rollback.
- A future region/profile or reverse conversion needs separate evidence,
  conversion rules, tests, and an ADR update; it cannot reuse this acceptance by
  assumption.

## Verification

- `rtk cargo test -p mh3g-save-convert`
- `rtk git diff --check`
- Later runtime acceptance: install only after all involved emulators stop,
  verify a real Cemu load/readback, then verify rollback before making a runtime
  compatibility claim.
