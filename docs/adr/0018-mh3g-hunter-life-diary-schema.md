# ADR 0018: Share the MH3G Hunter Life Diary schema across card views

- Status: Accepted
- Date: 2026-08-18
- Scope: Japanese MH3G 3DS to MH3G HD Wii U/Cemu conversion

## Context

The Hunter Life Diary is rendered from three user-facing entry points: the
player's personal guild card, received guild cards, and cards attached to
offline-hall partners. Releases through 0.0.22 relied on sparse MEOW operation
tables recovered from one reference save. Those tables converted only fields
that happened to be populated in that save. A populated event parameter in a
different diary row or received-card slot could therefore remain in 3DS
little-endian order and be read by Wii U as a very large big-endian value.

The reported Yoruaski sample reproduces the failure exactly. A stored value of
`40` (`28 00 00 00`) was read as `0x28000000` (`671088640`), and a value of
`1` (`01 00 00 00`) was read as `16777216`. Two Hunter Rank event parameters
similarly produced the reported `1442840577` and `1426063361` display values.

Four independently supplied official 3DS-to-Wii U transfer sets were compared
after removing their platform headers. The local evidence archive has SHA-256
`0262a4c5353deca35fa1220d89c87414383bab1454509d06d14d6414f7827712`;
the player files remain local and are never committed. Every informative field
agrees on this fixed record schema:

- personal table: payload `0x7B6C`, 10 records, stride `0xA0`;
- full received-card slot: relative `0x178`, 10 records, stride `0xA0`;
- relative `0x00..0x01`: packed day/month bytes, copied unchanged;
- relative `0x02`, `0x04`, `0x06`: independent `u16` values, LE to BE;
- relative `0x08..0x0B`: packed event descriptor, copied unchanged;
- relative `0x0C`, `0x10`, `0x14`, `0x18`, `0x1C`, `0x20`: independent
  event parameters stored as `u32`, LE to BE;
- text, participant names, and the remaining tail are copied unchanged.

`card1`, `card2`, and `card3` each contain 98 full `0xE00` slots. Experimental
CEC records embed three slots with the same shape. `cardbox` is a compact
index/summary format and does not contain this diary table.

## Decision

1. Keep the released 0.0.3-0.0.6 sparse replay unchanged for compatibility
   detection.
2. Add one source-based Hunter Life Diary record correction that owns the
   three `u16` and six `u32` field boundaries.
3. Apply that shared record correction to all 10 personal records, all 10
   records in every full received-card slot, and every CEC embedded slot.
4. Do not apply the schema to `cardbox`.
5. Add the same multibyte fields to compatibility repair. A field is repaired
   only while the current Wii U bytes still equal the selected historical
   converter output; a later Wii U change to any byte preserves the complete
   field as a conflict.
6. Preserve packed descriptors, names, text, and unrelated Wii U progress.

## Consequences

- The three guild-card entry points use the same numeric mapping instead of
  separate sparse offset coverage.
- Fresh conversion no longer produces powers-of-256 diary values.
- Existing converted `user#` and `card1`/`card2`/`card3` components can be
  repaired without overwriting later Wii U diary events.
- Existing converted CEC caches are not part of `repair-converted`; CEC uses
  the corrected schema when imported again from original 3DS inbox records.
- Deterministic tests cover all ten rows, the final received-card slot, the CEC
  slot path, whole-field conflict preservation, and idempotent repair.

## Verification boundary

The four official transfer sets prove the file-level field mapping. Synthetic
tests prove all converter and compatibility paths without committing private
saves. Screenshot values are reproduced from the supplied source bytes, but a
future release artifact still requires normal Windows packaging and player
runtime acceptance before the UI behavior is called Runtime Verified.
