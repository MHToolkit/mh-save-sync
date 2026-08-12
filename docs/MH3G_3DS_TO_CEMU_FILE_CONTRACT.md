# MH3G 3DS to Wii U/Cemu File Contract

[English](MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) | [简体中文](MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md)

This document records the **implemented CLI contract**, rather than an
inference from Cemu's save directory.  It applies only to the Japanese MH3G
`0x2B` profile accepted by `mh3g-save-convert`.

The CLI validates filenames, byte size, and save headers.  It does not infer
the title from a parent directory, so the operator or a future UI must choose
the actual Japanese MH3G HD Cemu save root deliberately.

## Source Material To Provide

Use the source data from one consistent 3DS/Nemessix/Azahar save state.  Do
not mix files copied at different times.

| Requested result | Required 3DS input | Current CLI command | Cemu destination that may change |
| --- | --- | --- | --- |
| Character, village/quest/event state, farm, hunting fleet, player data, and the slot-local offline-hunter cache | Exactly one loose `user1`, `user2`, or `user3` file | `convert <user#> --output <same user#>` | That same Cemu `user#` file only |
| Shared gallery/movie flags | Loose 3DS `system` plus an existing initialized Cemu `system` baseline | `convert-system system --output <existing Cemu system>` | Only the verified flag range inside Cemu `system`; all other target bytes are preserved |
| Received/local guild-card data and the card side of offline-hall partners | The complete 3DS extdata `user` directory containing all eight files listed below | `convert-extras --source-dir <extdata user> --output-dir <new empty staging dir>`, then `install-extras --staging-dir <staging> --target-dir <Cemu save dir> --groups guild-cards` | Converted files in staging only until an explicit complete-group transaction installs `card1`, `card2`, `card3`, and `cardbox`, with a manifest and retained prior bytes |
| Downloaded or created quests | The same complete 3DS extdata `user` directory | `convert-extras`, then `install-extras ... --groups quests` | Converted `quest1` through `quest4` in staging only until an explicit complete-group transaction installs them |
| StreetPass/Hunter Search cache | Optional 3DS CEC mailbox root with `InBox___/BoxInfo_____` and received message files | `convert-cec --dry-run`, then `convert-cec --write --experimental --expected-source-record-set-sha256 ... --expected-target-sha256 ...` | Cemu `cec` only, plus its transaction artifacts |

Typical locations are shown in the main README, but they are examples, not
hardcoded paths.  The logical source groups are:

```text
3DS title savedata (.../title/00040000/00048100/data/00000001/)
  user1 | user2 | user3 | system

3DS shared extdata (.../extdata/00000000/00000481/user/)
  card1  card2  card3  cardbox  quest1  quest2  quest3  quest4

3DS StreetPass/CEC (.../CEC/00048100/)
  InBox___/BoxInfo_____
  InBox___/_*                  # received MH3G message files
```

`00000481` is the MH3G ExtData root.  The required `convert-extras`
`--source-dir` is its `user` child, where all eight named files are immediate
children; do not pass the root, its `boss` child, or a hand-picked subset.
Likewise, the CEC `--source-dir` is the `00048100` mailbox root containing
`InBox___`, not the `InBox___` child alone.

## Archive Inputs

The CLI accepts ordinary filesystem files and directories only.  It does not
open ZIP, 7z, RAR, QQ, or browser archive previews and it never recursively
discovers a save from a parent directory.  Fully extract an archive to a normal
local directory first, then select the exact `user#`/`system` file, the exact
ExtData `user` directory, or the exact CEC `00048100` directory described
above.  Quote paths containing spaces.

The core slot command requires source and destination basenames to match.  For
example, a source called `user2` can write only a destination called `user2`;
it cannot be used to overwrite `user1` or an arbitrarily renamed file.

For a guarded `convert` write, bind the Dry Run's source SHA-256 plus one target
condition. An existing target uses `--expected-target-sha256`; a new output
uses `--expected-target-absent`. Those target conditions are mutually
exclusive. `convert-system` does not permit a new target: its write requires
both the source and existing-target SHA-256 from the immediately preceding Dry
Run. The transaction obtains its lock and checks the condition again.

## Exact Component Groups

### Required Core Slot

For a normal character migration, provide exactly one source `user#` file and
select the same numbered Cemu destination:

| Source | Accepted source size | Cemu output | Output size | Required |
| --- | ---: | --- | ---: | --- |
| `user1` | `0x8A00` | `user1` | `0x8A24` | Choose one slot |
| `user2` | `0x8A00` | `user2` | `0x8A24` | Choose one slot |
| `user3` | `0x8A00` | `user3` | `0x8A24` | Choose one slot |

The selected `user#` is the only mandatory input for the core converter.  It
includes the main character state and the slot-local offline-hunter roster and
candidate/cache data.  `convert` never automatically opens `system`,
`card*`, `quest*`, `cec`, or another `user#`.

### Compatibility Repair for an Older Conversion

Compatibility repair is separate from a new conversion. It has five
independently authorized domains; selecting or completing one never authorizes,
writes, or rolls back another:

| Repair domain | Original 3DS input | Read-only current Wii U/Cemu authority | Independent output | Command |
| --- | --- | --- | --- | --- |
| Core slot | Exact `user1`, `user2`, or `user3` | Same-named `user#` | Same-named `user#` | `repair-converted` |
| Guild cards | ExtData `user` directory containing `card1`-`card3`, `cardbox` | Directory containing initialized matching files | Directory containing initialized matching files | `repair-extras --group guild-cards` |
| Quests | ExtData `user` directory containing `quest1`-`quest4` | Directory containing initialized matching files | Directory containing initialized matching files | `repair-extras --group quests` |
| Shared gallery/movie state | Exact 3DS `system` | Exact initialized Cemu `system` | Exact `system` path | `repair-system` |
| StreetPass/CEC cache | Exact MH3G CEC mailbox directory | Exact initialized Cemu `cec` | Exact `cec` path | `repair-cec` |

Every domain has its own Dry Run fingerprint, write authorization, transaction
manifest, and rollback. Source, current, and output path values are never
copied or cascaded between controls. The native interfaces accept a core
`user#` file or its direct parent directory; they do not recursively scan a
3DS SD root, a Cemu MLC, or an archive. `system` and `cec` are exact files.
Cards and quests share the same three ExtData directory selectors because they
are siblings in one physical directory, but each group still has a separate
Dry Run, write button, manifest, and rollback.

The core contract is:

```text
mh3g-save-convert repair-converted <3DS-user#> --current <current-Cemu-user#> \
  --output <repaired-Cemu-user#> \
  [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] \
  [--dry-run | --write --expected-source-set-sha256 <SHA256> \
    --expected-current-set-sha256 <SHA256> --expected-output-set-sha256 <SHA256> \
    --expected-preview-sha256 <SHA256>]
```

The original 3DS slot and `--current` slot are read-only merge inputs;
`--output` is the only payload path that may be written. All three must name
the same slot. Current Cemu data is authoritative for continued gameplay. A
known historical field is replaced only when the current value still equals
the selected old converter output; later Wii U changes are preserved and
reported as conflicts. Omitting `--output` retains legacy CLI in-place behavior
only; both native UIs always display and pass a separate output.

The two ExtData domains use:

```text
mh3g-save-convert repair-extras --group <guild-cards|quests> \
  --source-dir <3DS-ExtData-user> --current-dir <current-Cemu-save-dir> \
  --output-dir <initialized-output-Cemu-save-dir> \
  [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] [--dry-run | --write ...]
```

The selected output group must already be initialized and complete:
`card1`, `card2`, `card3`, and `cardbox` for `guild-cards`; `quest1`,
`quest2`, `quest3`, and `quest4` for `quests`. Partial-file repair is refused.
Guild-card fields use the historical three-way merge. Quest payloads have no
reviewed historical defect map, so the quest repair transaction validates the
complete group and copies the **current Wii U bytes** to output exactly; it
does not restore original 3DS quest data.

The shared `system` repair uses:

```text
mh3g-save-convert repair-system <3DS-system> --current <current-Cemu-system> \
  --output <repaired-Cemu-system> [--dry-run | --write ...]
```

Only the verified gallery/movie bit range is unioned from the source into the
current Cemu authority; all other current bytes, including other-slot shared
state, are preserved. The output file may be new, but its parent directory must
exist. This is intentionally independent from both core and ExtData repair.

Experimental CEC repair uses:

```text
mh3g-save-convert repair-cec --source-dir <3DS-MH3G-CEC-mailbox> \
  --current <current-Cemu-cec> --output <repaired-Cemu-cec> \
  [--slot <N>] [--dry-run | --write --experimental ...]
```

It replaces exact recognized historical records, preserves unrelated current
slots, and otherwise fills empty slots only. CEC remains experimental and its
write requires explicit acknowledgement. Its output may be new, but both
current and output basenames must be `cec`.

For core, guild cards, and quests, Dry Run reports `exact`,
`compatible-range`, `ambiguous`, or `unknown`. An `ambiguous` result requires
an explicit, non-contradicted `--from-version` and a new Dry Run; `unknown` is
refused. The native workflow reuses one manually selected historical revision
for these three domains because they originated from one converter run, while
each domain still produces independent evidence and authorization. `system`
and CEC do not use a historical converter revision.

`repair-converted --source-extdata-dir` remains accepted for older scripts and
can still coordinate core plus cards through one legacy manifest. New native
UI flows do not use this coupled option. Prefer the domain-scoped commands and
their matching rollback routes: `rollback-repair` for core,
`rollback-extras` for one ExtData group, `rollback` for `system`, and
`rollback-cec` for CEC. `phrase1` through `phrase3` remain outside repair.

### Optional Shared `system`

`system` is a separate shared component, not an implicit side effect of
`convert user#`.

| Source | Accepted source size | Cemu output | Output size | Command |
| --- | ---: | --- | ---: | --- |
| `system` | `0x3000` | `system` | `0x3024` | `convert-system` |

Provide it only when the migration explicitly includes the housekeeper
gallery/movie history. `system` is shared across all three character slots and
also holds settings that are not owned by the selected slot. The command must
therefore receive both a 3DS source and an existing initialized Cemu target. It
recognizes the `0x3000` 3DS and `0x3024` Cemu profiles, bitwise-unions only the
verified gallery/movie flag range (Cemu file offsets `0x68..0x77`), and
preserves the Cemu header and every other target byte. A missing or malformed
Cemu baseline is rejected. Omitting this transaction leaves Cemu `system`
untouched, so a core-slot conversion alone cannot fill missing gallery entries.

### Optional Shared Extdata

The current CLI has one all-or-nothing **conversion input** group.  Its source
directory must contain every one of these files, even when the final Cemu
installation will use only a subset:

| 3DS source filename | Source size | Generated Cemu filename | Generated size | Content group |
| --- | ---: | --- | ---: | --- |
| `card1` | `0x58000` | `card1` | `0x58024` | Guild cards |
| `card2` | `0x58000` | `card2` | `0x58024` | Guild cards |
| `card3` | `0x58000` | `card3` | `0x58024` | Guild cards |
| `cardbox` | `0x30000` | `cardbox` | `0x30024` | Guild-card storage |
| `quest1` | `0x29000` | `quest1` | `0x29024` | Downloaded/created quests |
| `quest2` | `0x29000` | `quest2` | `0x29024` | Downloaded/created quests |
| `quest3` | `0x29000` | `quest3` | `0x29024` | Downloaded/created quests |
| `quest4` | `0x29000` | `quest4` | `0x29024` | Downloaded/created quests |

`convert-extras` reads all eight originals and generates all eight outputs.
It has no `--components` or per-file conversion mode.  The separate
`install-extras` command can install one or both **complete** groups from a
validated staging directory into an initialized Cemu target.  It still must
collect the complete ExtData directory for conversion and may never install a
single `card#` or `quest#` file.

`card1`, `card2`, `card3`, and `cardbox` are the supported guild-card group.
The three `card#` files share a full received-card layout; `cardbox` has its
own compact layout and conversion table.  Do not raw-copy any of them from 3DS
to Cemu.

`quest1` through `quest4` are a separate quest group.  They are converted by
the same staging command because its input contract is fixed, but they are not
the guild-card dependency.

`--reset-guild-cards` is not normal conversion.  When explicitly passed, it
creates empty Cemu `card1`, `card2`, `card3`, and `cardbox` files and discards
the source guild-card data; the quest outputs remain normally converted.

### Installing Staged ExtData

`install-extras` is the only supported overwrite path for staged ExtData:

```text
mh3g-save-convert install-extras [--dry-run | --write] \
  --staging-dir <staging dir> --target-dir <initialized Cemu save dir> \
  --groups <guild-cards,quests>
```

The staging directory must contain all eight converted files. `guild-cards`
always means `card1`, `card2`, `card3`, and `cardbox`; `quests` always means
`quest1` through `quest4`. The target must already be an initialized MH3G Cemu
save directory containing the selected component names. A Dry Run reports both
the staging-set and target-set SHA-256 values. Supply those exact values to the
immediate `--write` with `--expected-staging-set-sha256` and
`--expected-target-set-sha256`; the write rechecks them while it holds its
directory lock. It creates a manifest-bound recovery transaction and preserves
the previous target bytes before replacing any selected component.

### Optional StreetPass/CEC

CEC is neither part of `user#` nor part of `card*`, and it is not required for
the normal guild-card/offline-partner path.  It is an experimental, independent
cache import:

| 3DS input | Cemu output | Write condition |
| --- | --- | --- |
| CEC root's non-empty received MH3G records in `InBox___/_*` | `cec` | `convert-cec --write --experimental` plus the `source_record_set_sha256` and `target_sha256_before` from its immediate Dry Run |

`convert-cec` requires `InBox___/BoxInfo_____`; it deliberately ignores
`OutBox__` records because those describe the source hunter's outgoing
transmission. It reports an order-independent `source_record_set_sha256` and a
`target_sha256_before`. A write requires both values as
`--expected-source-record-set-sha256` and `--expected-target-sha256`; the
target value represents the canonical empty Cemu container if `cec` is absent.
The write obtains the target lock, re-reads both inputs, verifies both values,
and only then creates the cache. It does not write a card file or a `user#`
file.

`inspect-cec` is broader and read-only: it reports both `InBox___` and
`OutBox__`, and can optionally read a `user#` only to locate a card anchor.

## Guild Cards and Offline-Hall Partners

The supported file-level dependency is:

```text
matching user#
  + card1 + card2 + card3 + cardbox
  = normal guild-card and offline-hall-partner migration set
```

`user#` stores the six offline-hunter roster/cache records and candidate
anchors.  The converter transforms their platform-specific fields.  The guild
card components store the associated card bodies.  Regression tests prove that
the eight-byte anchors preserved in `user#` match anchors in converted card
slots.  Therefore a migration intended to retain already received cards and
their offline-hall partners must retain both sides: the chosen `user#` and all
four card components.

There is no evidence-backed safe rule for selecting only one `card#` file for
that result.  Treat all four as one installation group.  Conversely, CEC is
not a prerequisite for these existing card/partner records; its raw received
record import remains explicitly experimental.

## Read/Write Boundary by Command

| Command | Reads | Writes with default/dry-run | Writes with `--write` |
| --- | --- | --- | --- |
| `inspect <file>` | One named source file | Nothing | N/A |
| `inspect-progress <user#> [--target <user#>]` | Source and optional target slots | Nothing | N/A |
| `inspect-events <user#> [--target <user#>]` | Source and optional target slots | Nothing | N/A |
| `convert <user#> --output <same user#>` | Source slot; existing target and prior transaction records only when installing | Nothing | Named target slot plus core transaction artifacts below |
| `repair-converted <3DS-user#> --current <current-Cemu-user#> --output <repaired-Cemu-user#>` | Original 3DS slot, read-only current Cemu slot, and independent output state | Nothing | Only the output-side same-named `user#` fields proven to need repair, plus a core compatibility manifest; the current reference remains unchanged |
| `repair-extras --group guild-cards ...` | Complete original, current, and output guild-card groups | Nothing | Only the complete output `card1`-`card3`/`cardbox` group and its independent ExtData transaction |
| `repair-extras --group quests ...` | Complete original, current, and output quest groups | Nothing | Copies the complete current `quest1`-`quest4` group to the independent output and records its own transaction; original quest payload is never restored |
| `repair-system system --current system --output system` | Original 3DS `system`, read-only current Cemu `system`, and independent output state | Nothing | Only the verified gallery/movie union based on current Cemu bytes, plus a core-style manifest |
| `repair-cec --source-dir ... --current cec --output cec` | 3DS CEC mailbox, read-only current Cemu `cec`, and independent output state | Nothing | Exact historical record replacements/empty-slot additions in output `cec`, plus its CEC manifest; requires `--experimental` |
| `convert-system system --output <existing Cemu system>` | 3DS source `system` and existing initialized Cemu target on Dry Run and write | Nothing | Only the verified gallery/movie flag union in the named target, plus the same transaction artifact pattern |
| `convert-extras --source-dir ... --output-dir ...` | All eight extdata files | Nothing, and no output directory is created | Only the eight generated files under `output-dir` |
| `install-extras --staging-dir ... --target-dir ... --groups ...` | Complete staged ExtData set and selected initialized target group(s) | Nothing | Only the selected complete Cemu group(s), plus one manifest-bound ExtData recovery transaction below |
| `inspect-cec --source-dir ... [--target cec] [--source-slot user#]` | CEC `InBox___` and `OutBox__`; optional `cec` and optional user slot | Nothing | N/A |
| `convert-cec --source-dir ... --target cec` | Received `InBox___` records and the existing `cec`, if any | Nothing | `cec` plus CEC transaction artifacts; requires `--experimental` and both expected Dry Run hashes |
| `rollback` | Its controlled core/system manifest, target, and backup | N/A | Restores or removes only the manifest-bound core/system target and removes its transaction artifacts |
| `rollback-repair` | Compatibility coordinator manifest and its core/ExtData child manifests | N/A | Rolls back every compatibility-repair child transaction in the controlled order |
| `rollback-extras` | Its controlled ExtData transaction manifest, selected target group(s), and retained prior bytes | N/A | Restores only the manifest-bound complete group(s) |
| `rollback-cec` | Its controlled CEC manifest, target, and backup | N/A | Restores or removes only the manifest-bound CEC target and removes its transaction artifacts |

For `convert` and `convert-system`, a successful write uses a same-directory
temporary file and atomic rename.  When an old target exists, it creates a
hash-addressed backup.  The persistent managed files are:

```text
.<user#|system>.mh3g-backup-<previous-sha256>       # only if target existed
.<user#|system>.mh3g-install.json
.<user#|system>.mh3g-install-history-<sha256>.json # possible on reinstall
```

The short-lived `.<user#|system>.mh3g-install.lock` and temporary file are
removed after the transaction. `convert-extras` deliberately has no target
backup, manifest, or overwrite path: it refuses `--write` if any of its eight
named staging outputs already exists. Use a fresh staging directory and
compare the reported hashes.

`install-extras` provides the controlled install step. It writes a unique
hidden `.mh3g-extra-transaction-.../` directory below the target containing
the returned `.mh3g-extra-recovery.json` manifest and retained prior component
bytes. The advisory lock is held only during an operation, while the regular
`.mh3g-extra-install.lock` pathname intentionally remains as a stable lock
inode. `rollback-extras` accepts only the returned manifest and restores the
complete group(s) named by it; it does not accept individual component paths.
The transaction directory and manifest remain afterward as audit evidence,
even though every selected payload byte is restored.

For experimental CEC, the equivalent persistent names are:

```text
.cec.mh3g-backup-<previous-sha256>  # only if cec existed
.cec.mh3g-install.json
```

## Files That Are Not Automatically Modified

The converter has no recursive "convert this whole save directory" command.
Unless a command explicitly names a path above, it does not modify it.  In
particular:

- Converting `user2` does not modify `user1`, `user3`, `system`, `card1`,
  `card2`, `card3`, `cardbox`, `quest1` through `quest4`, or `cec`.
- Converting `system` does not modify any `user#`, `card*`, `quest*`, or `cec`.
- `convert-extras` does not modify any source file, user slot, `system`, or
  `cec`; it writes only its explicit staging outputs.
- `install-extras` does not modify source files, any `user#`, `system`, `cec`,
  or a non-selected ExtData group; it changes only the selected complete target
  group(s) and their controlled transaction artifacts.
- `convert-cec` does not modify `user#`, `system`, `card*`, or `quest*`.
- `repair-converted` in the native domain-scoped flow changes only its explicit
  output `user#`. The legacy `--source-extdata-dir` option is the sole coupled
  exception retained for old CLI scripts.
- `repair-extras` changes only the selected complete output group. Guild-card
  and quest runs do not authorize or roll back each other, even when their
  directory selectors contain the same path values.
- `repair-system` changes only its explicit output `system`; `repair-cec`
  changes only its explicit output `cec`.
- `phrase1`, `phrase2`, and `phrase3` are not enumerated by any converter
  command and are not read or written by the MH3G conversion implementation.
- The source 3DS save files are always read-only from this CLI's perspective.

The only exceptions to the named payload files are the adjacent backup,
manifest, history, lock, and temporary transaction artifacts described above.

## Implementation Evidence

This contract is derived from the executable implementation and tests:

- `crates/mh3g-save-convert/src/main.rs`: CLI parameters, same-name slot
  validation, independent `repair-converted`/`repair-extras`/`repair-system`/
  `repair-cec` authorization and manifests, the eight-file `convert-extras`
  loop, and dry-run non-write behavior.
- `crates/mh3g-save-convert/src/converter.rs`: the exact eight extdata names,
  per-component validation, guild-card versus quest conversion behavior, and
  the source-read-only pure slot conversion.
- `crates/mh3g-save-convert/src/profile.rs`: accepted `user1`/`user2`/`user3`
  and `system` basenames and the source/Cemu byte-size profiles.
- `crates/mh3g-save-convert/src/transaction.rs`: atomic core/system install,
  backup, manifest, history, lock, and rollback boundaries.
- `crates/mh3g-save-convert/src/cec.rs`: inbox-only experimental CEC import,
  exact historical-record replacement for repair, `cec` target validation,
  and CEC backup/manifest behavior.
- `crates/mh3g-save-convert/tests/cli.rs` and
  `crates/mh3g-save-convert/src/converter.rs` tests: dry-run non-write,
  cross-slot rejection, independent output/current preservation, complete-group
  quest copying, system shared-byte preservation, CEC outbox rejection, and
  offline-hunter/card-anchor regression coverage.
