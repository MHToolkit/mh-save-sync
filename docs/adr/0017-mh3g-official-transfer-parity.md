# ADR 0017: Use paired official transfers for MH3G field boundaries

- Status: Accepted
- Date: 2026-08-16
- Scope: Japanese MH3G 3DS to MH3G HD Wii U/Cemu conversion

## Context

Several fields that look numeric are actually packed byte state. Broad endian
swaps can therefore produce plausible levels while corrupting the value later
consumed by the game. Likewise, hunt counters are not proof that a monster is
marked discovered in Hunter's Notes.

Five independently supplied `user#` pairs were compared byte-for-byte after
removing their platform container headers. They came from one local evidence
archive whose SHA-256 is
`0262a4c5353deca35fa1220d89c87414383bab1454509d06d14d6414f7827712`.
The archive and player saves remain local evidence and are never committed.

The paired files agree on these boundaries:

- each Cha-Cha/Kayamba record starts at payload `0x6F44`, with stride `0x148`;
- relative `0x000..0x003` is one endian-converted `u32`;
- relative `0x004..0x0DD` is a sequence of endian-converted `u16` lanes;
- relative `0x0DE..0x13F` is a byte-packed mask/mastery block and is copied
  unchanged;
- each personal monster state is at `0x81B4 + row * 10 + 8`;
- received-card and CEC card states use the same semantic mapping at their own
  slot-relative offsets;
- discovery/crown state is a source-owned bit permutation. Non-zero slay or
  capture counts do not synthesize discovery.

Static inspection of the Wii U executable and its Japanese item-name resource
also corrects an earlier schema label. Payload `0x65C4..0x6683` is not 48
monster records; it is 48 endian-sensitive `u32` words forming a 1536-bit
item-acquisition bitset. The Wii U accessor selects `item_id >> 5` and then
tests `item_id & 31`. The Deviljho book is item `0x4F8`, so it is word 39,
bit 24. In the Yoruaski source, that word is `0xFFFF7FFE`: bytes
`FE 7F FF FF` on 3DS must become `FF FF 7F FE` on Wii U.

All five official-transfer pairs agree byte-for-byte on the complete 192-byte
bitset after this LE-to-BE word conversion. Releases through 0.0.16 left these
words untouched, which made Wii U read the Yoruaski sample as `0xFE7FFFFF` and
cleared the Deviljho-book bit. Current conversion (introduced in 0.0.17)
preserves the bit required by the ninth Hunter's Notes page. No item-ID
remapping or adjacent-field rewrite is supported by the evidence; native game
acceptance must still confirm that the intended output file and slot were
actually loaded.

The same five pairs also expose a second, independent monster-list condition.
Payload `0x5760..0x577B` is a 28-byte packed state array and is copied verbatim
by the official transfer. Historical conversion treated part of its tail as
`u16` lanes. For Yoruaski this changed source tail `FF FF FF 00` into
`FF FF 00 FF`, clearing byte `0x577A`. The acquired Deviljho-book bit and the
personal Deviljho record can therefore both be correct while the runtime list
still omits Deviljho. Current conversion restores the complete byte-packed
array; compatibility repair handles it one byte at a time so later Wii U
progress in another byte is not reverted.

The displayed hunt totals are a third, separate schema. There are 86 valid
monster IDs (`0x00..0x55`), with one `u16` slay counter per ID at payload
`0x5784 + id * 2` and one `u16` capture counter per ID at
`0x5884 + id * 2`. All five official-transfer pairs preserve each numeric
value while changing its encoding from 3DS little-endian to Wii U big-endian.
Static inspection of both executables independently confirms the `0x56` loop
limit and the `0x270F` (9999) saturation used by the counter update paths.
Historical conversion missed the slay lanes for IDs `0x1A`, `0x1B`, and
`0x1C` (Giggi, Aptonoth, and Popo). Wii U consequently interpreted ordinary
values such as `0x0584` as `0x8405` and clamped the displayed total to 9999.
This count schema does not overlap the packed monster-list state.

Deterministic replay shows that the historical and current algorithms differ
at slay IDs `26, 27, 28, 84` and capture IDs
`21..23, 26..33, 76..84`. Only the first three slay lanes are non-zero in all
five paired samples and directly reproduce the reported symptom. Compatibility
repair nevertheless covers the exact 24 differing lanes so future non-zero
values are corrected, while excluding already-identical lanes from revision
detection scores.

## Decision

Current conversion layers the official-transfer corrections after the closed
0.0.3-0.0.6 historical replay:

1. copy the confirmed Shakalaka packed block without byte swapping;
2. derive personal, received-card, and CEC Hunter's Notes visibility only from
   the source state byte;
3. keep one shared state-only mapping helper for all three storage locations;
4. retain the former hunt-counter inference only inside historical replay so
   compatibility detection remains byte-reproducible;
5. include every corrected state byte and packed mask pair in compatibility
   repair's field list, preserving later Wii U edits outside those fields;
6. convert all 48 item-acquisition words independently and include them in
   compatibility repair under their actual item-bitset semantics;
7. preserve the 28-byte packed monster-list state verbatim and repair its
   historical output at byte granularity;
8. reassert all 86 valid slay and capture counters from the original source as
   independent LE-to-BE `u16` fields. Historical replay remains unchanged;
   compatibility repair includes only lanes whose historical output differs
   from the current rule and preserves any later Wii U value at whole-field
   granularity.

## Consequences

- Lamp Mask mastery no longer becomes zero merely because its two packed bytes
  were reversed.
- Hidden source monsters no longer create extra guide pages after conversion.
- The Deviljho book and other acquired-item unlocks no longer disappear merely
  because their 32-bit word was interpreted in the wrong byte order.
- Giggi, Aptonoth, and Popo hunt totals no longer become saturated 9999 values
  because their two bytes were left in 3DS order.
- Received cards and offline-hall partners cannot drift from the personal
  Hunter's Notes mapping.
- Existing 0.0.5/0.0.6 saves remain detectable and can be repaired without
  overwriting unrelated post-conversion progress.

## Shared `system` boundary

The physical save layout contains one title-wide `system` beside all three
`user#` slots, not separate per-slot system files. This proves physical file
ownership, but it does not prove whether individual bits internally encode
slot-specific semantics. Gallery/movie state must therefore not be presented
as reliably attributable to a character slot. The current conservative merge
remains a separate transaction that requires both a 3DS source and an
initialized Cemu target.

This official-pair audit does **not** upgrade the exact gallery bit mapping to
runtime-verified status: the archive contains only one strict source/target
`system` pair, and its observed bytes do not by themselves prove a general
bit remap. A controlled single-unlock before/after capture is still required
before broadening the mapped range or claiming complete official parity.

## Residual parity backlog

The paired-file comparison is intentionally not being presented as whole-file
parity. After the mask and Hunter's Notes corrections, each of the five core
slots still differs from its paired Wii U file by 200 to 274 payload bytes.
The remaining high-confidence clusters include fleet/roster fields, companion
record prefixes, offline-hunter records, and mixed record tables whose complete
schemas are not yet proven.

Received-card comparison also found independent, repeatable gaps outside the
Hunter's Notes state byte: compact-equipment tail scalars, four slot-local
`u32` fields, summary records after the formerly assumed 33-record boundary,
and three fixed-width `cardbox` values. `quest1` through `quest4` matched their
paired files byte-for-byte. These residuals need their own compatibility field
specifications and regression fixtures; they are not silently folded into this
symptom-focused change.

## Verification boundary

Synthetic tests cover current conversion, historical replay, compatibility
repair, received-card conversion, and CEC conversion. Local scripts compare
the corrected regions against the five uncommitted official pairs. Game UI
behavior and the exact semantics of every byte inside the packed mask block
remain runtime acceptance items.
