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

### Shared extdata and guild cards

`card1`, `card2`, `card3`, `cardbox`, and `quest1` through `quest4` are shared
3DS extdata components. They are not embedded in a `user#` slot and are outside
the slot-transform differential proof above.

The official Japanese transfer program has one component-name table containing
the three user slots, `card1`/`card2`/`card3`, `cardbox`, and `quest1` through
`quest4`. Its card transfer states copy `0x58000` bytes for each `card*` file
and `0x30000` bytes for `cardbox`; the quest states copy `0x29000` bytes. No
payload transformation is present in those states. This is independently
corroborated by a local Japanese 3DS `card2` and its Cemu counterpart whose
payloads have the same SHA-256
`6af2f63481dce37f692c0ae1df71d1e3244bb53b2009f3d59b9891e6bc1cbb33`.

`convert-extras` consequently preserves every valid non-empty `card*` payload
and changes only its 3DS four-byte outer container into Cemu's 40-byte
wrapper. `--reset-guild-cards` remains available only as an explicit
destructive recovery option: it writes native empty Cemu components and drops
both local and received cards.

The same official program initializes and uses the 3DS `cecd:u` mailbox for
MH3G (`0x00048100`). CEC/StreetPass messages are outside the eight extdata
components. The observed MH3G outgoing message has a `0xD80` header and a
`0x2A08` body; its body after the first eight bytes is exactly the `0x2A00`
record-sized candidate used by Cemu's fixed-slot geometry. `inspect-cec`
reports that candidate and any source-slot anchor matches. Cemu's Japanese
`cec` container has a 40-byte outer wrapper, a 0x1FC-byte cache prefix, and 50
consecutive `0x2A00` slots. An isolated Cemu process-memory canary established
that a source body record copied byte-for-byte into the first slot reaches
guest memory while the title reads `cec`. `convert-cec` therefore imports each
non-empty MH3G record into an empty fixed slot and preserves the outer wrapper,
cache prefix, and existing occupied slots. It does not synthesize unknown
index/validity metadata or overwrite a slot.

This establishes a file-level CEC candidate only, not a Runtime Verified
StreetPass migration. Runtime acceptance must still verify the guild-card UI,
receiving a new card, and quest dispatch.

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

### Japanese save differential proof (2026-07-19)

The file-level differential check used `3usavetools` commit
`d20fea5d98d5c465841c8e5626dae6709622354a` with Python `3.12.10`. The pinned
`converter/save_indices.py` SHA-256 was
`0753baafad37147cb4701b7315a9deb9055ff699f444d55fce537b4e1ae35deb`.
The Rust converter was built with Cargo `1.95.0` and Rust `1.95.0` at commit
`98c2610`.

The source snapshot was `0x8A00` bytes with SHA-256
`5da7b0a8566aa6a77288cc43de0ab3538f5aec30031c3794880a88990b20b70c`.
The upstream reference produced SHA-256
`87ff5751b6b78d7a8e8905048075614a017678d98bddc8331c2f86d3a2401f30`.
Changing only reference byte index 39 from `0x2C` to the locally verified
Japanese `0x2B` produced an `0x8A24`-byte reference with SHA-256
`59aed8e517c1f18127d7c90c2944572e6058ce7686592a6af9564a12466bf6ad`.

The reproducible command sequence, with save-bearing paths represented by
shell variables, was:

```bash
rtk cp "$NEMESSIX_SOURCE" "$SOURCE"
rtk proxy python3 /tmp/3usavetools-031/convert_to_wiiu.py "$SOURCE" "$REFERENCE_WESTERN"
rtk cp "$REFERENCE_WESTERN" "$REFERENCE_JP"
rtk proxy python3 -c 'import sys; from pathlib import Path; p=Path(sys.argv[1]); b=bytearray(p.read_bytes()); assert len(b)==0x8A24; assert b[39]==0x2C; b[39]=0x2B; p.write_bytes(b)' "$REFERENCE_JP"
rtk cargo run -q -p mh3g-save-convert -- inspect "$SOURCE"
rtk cargo run -q -p mh3g-save-convert -- convert "$SOURCE" --output "$STAGE/user2" --write
rtk cmp "$STAGE/user2" "$REFERENCE_JP"
rtk proxy shasum -a 256 "$SOURCE" "$STAGE/user2" "$REFERENCE_JP"
rtk cargo run -q -p mh3g-save-convert -- inspect "$STAGE/user2"
rtk cargo run -q -p mh3g-save-convert -- rollback --manifest "$STAGE/.user2.mh3g-install.json"
```

The production CLI reported profile `JpCemu`, size `0x8A24`, and output
SHA-256 `59aed8e517c1f18127d7c90c2944572e6058ce7686592a6af9564a12466bf6ad`.
`cmp` confirmed byte-for-byte equality with the patched Japanese reference.
The version-1 manifest was derived as `.user2.mh3g-install.json`, recorded an
initially absent target and no backup, and bound the expected source and output
hashes. Rollback removed both the staged target and manifest, restoring the
initially absent state. The original source hash remained unchanged.

Money, playtime, item-box, award, monster-log, arena-record, and the main
slot's guild-card-related fields have labels in the pinned reference and were
differentially verified. This does not establish a conversion for the separate
`card1`/`card2`/`card3`/`cardbox` extdata payloads. The player-header structure,
equipment box, and Moga points have no independently established field mapping
in this validation; they have only whole-file differential parity and the
converter's byte-preservation contract. All checkpoint categories remain
semantically unverified until Cemu runtime acceptance. No player content or
save bytes were recorded.
