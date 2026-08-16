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
   repair's field list, preserving later Wii U edits outside those fields.

## Consequences

- Lamp Mask mastery no longer becomes zero merely because its two packed bytes
  were reversed.
- Hidden source monsters no longer create extra guide pages after conversion.
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
