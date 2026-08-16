# ADR 0016: Merge MH3G shared-system gallery flags conservatively

- Status: Accepted; evidence boundary amended by ADR 0017
- Date: 2026-08-10
- Scope: Japanese MH3G 3DS `system` to MH3G HD Wii U/Cemu `system`

## Context

MH3G stores candidate housekeeper gallery/movie state in the separate `system`
component rather than in the converted `user1`, `user2`, or `user3` core
component. The physical layout exposes one `system` beside all three character
slots, rather than separate per-slot system files, and it also contains settings
and records unrelated to the slot currently being migrated. Current evidence
does not prove whether individual bits internally encode slot-specific
semantics.

Converter versions through 0.0.16 exposed `convert-system` as a complete 3DS
payload conversion followed by replacement of the Cemu target. Although that
operation had backup, manifest, hash preconditions, and rollback, replacing
the entire shared payload could discard Wii U settings or state contributed by
another character slot.

The Japanese files have distinct validated containers:

- 3DS `system`: `0x3000` bytes, `JpThreeDsSystem`;
- Wii U/Cemu `system`: `0x3024` bytes, `JpCemuSystem`.

Community file-level research identifies candidate gallery unlock booleans at
Cemu file offsets `0x68..0x77` and reports that gallery state is shared between
profiles. Later paired official-transfer audit confirms the title-wide
physical `system` ownership but does not prove a one-to-one bit mapping for
this range: the single strict source/target `system` pair does not equal a raw
endian conversion or union. Therefore this remains a conservative,
synthetic-tested opt-in mapping rather than complete official-transfer or
game-runtime proof.

## Decision

`convert-system` becomes a two-input merge:

1. require one valid Japanese 3DS `system` source;
2. require one existing, initialized Japanese Wii U/Cemu `system` target;
3. decode four 3DS little-endian flag words at logical payload `0x40..0x4F`;
4. decode the matching four Cemu big-endian words;
5. write their bitwise union back in Wii U byte order;
6. preserve the current Cemu header and every byte outside that 16-byte range.

There is no implicit or explicit new-`system` export. `--write` requires both
the source SHA-256 and target SHA-256 from the immediately preceding Dry Run.
The installer rechecks the target hash under its component lock, creates a
hash-addressed backup, atomically replaces the target, and publishes the
existing manifest used by `rollback`.

The lower-level complete container/endian conversion remains available to
unit tests and format analysis, but it is no longer the public
`convert-system` installation behavior.

## Consequences

- Existing Wii U bits in the mapped range are retained while mapped 3DS bits
  are added.
- Other-slot settings and unknown shared records are not overwritten.
- A missing, malformed, or wrongly selected Cemu `system` fails closed.
- A user must start MH3G HD once to initialize a target before migrating
  gallery/movie flags.
- This operation intentionally does **not** claim to migrate every unknown
  field in the 3DS `system` payload.

## Migration and rollback

Users of 0.0.16 or earlier should not rerun the old complete-file
`convert-system` operation. Select the original 3DS `system` and the current
Cemu `system`, run the new Dry Run, then write with both reported hashes.

Every write retains the previous Cemu bytes in the standard
`.system.mh3g-backup-<sha256>` file. `rollback --manifest
<.system.mh3g-install.json>` restores that exact baseline.

## Verification boundary

Deterministic tests prove profile recognition, endian-aware flag union,
preservation outside `0x68..0x77`, target/hash refusal, transaction backup, and
manifest behavior. Supplied transfer files are used only as local, uncommitted
comparison evidence. The precise flag semantics and game UI/runtime behavior
remain unverified until controlled single-unlock before/after captures exist.
