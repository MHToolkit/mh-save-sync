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
| Shared system data | Loose `system` file | `convert-system system --output system` | Cemu `system` only |
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

### Optional Shared `system`

`system` is a separate shared component, not an implicit side effect of
`convert user#`.

| Source | Accepted source size | Cemu output | Output size | Command |
| --- | ---: | --- | ---: | --- |
| `system` | `0x3000` | `system` | `0x3024` | `convert-system` |

Provide it only when the migration explicitly includes shared system data.
The command reads and writes only the explicitly named `system` paths; it does
not alter any `user#` slot.

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
| `convert-system system --output system` | Source `system`; existing target and prior transaction records only when installing | Nothing | Named `system` plus the same transaction artifact pattern |
| `convert-extras --source-dir ... --output-dir ...` | All eight extdata files | Nothing, and no output directory is created | Only the eight generated files under `output-dir` |
| `install-extras --staging-dir ... --target-dir ... --groups ...` | Complete staged ExtData set and selected initialized target group(s) | Nothing | Only the selected complete Cemu group(s), plus one manifest-bound ExtData recovery transaction below |
| `inspect-cec --source-dir ... [--target cec] [--source-slot user#]` | CEC `InBox___` and `OutBox__`; optional `cec` and optional user slot | Nothing | N/A |
| `convert-cec --source-dir ... --target cec` | Received `InBox___` records and the existing `cec`, if any | Nothing | `cec` plus CEC transaction artifacts; requires `--experimental` and both expected Dry Run hashes |
| `rollback` | Its controlled core/system manifest, target, and backup | N/A | Restores or removes only the manifest-bound core/system target and removes its transaction artifacts |
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
bytes. Its `.mh3g-extra-install.lock` is short-lived. `rollback-extras` accepts
only that returned manifest and restores the complete group(s) named by it; it
does not accept individual component paths.

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
- `phrase1`, `phrase2`, and `phrase3` are not enumerated by any converter
  command and are not read or written by the MH3G conversion implementation.
- The source 3DS save files are always read-only from this CLI's perspective.

The only exceptions to the named payload files are the adjacent backup,
manifest, history, lock, and temporary transaction artifacts described above.

## Implementation Evidence

This contract is derived from the executable implementation and tests:

- `crates/mh3g-save-convert/src/main.rs`: CLI parameters, same-name slot
  validation, the eight-file `convert-extras` loop, dry-run behavior, and its
  new-output-directory refusal.
- `crates/mh3g-save-convert/src/converter.rs`: the exact eight extdata names,
  per-component validation, guild-card versus quest conversion behavior, and
  the source-read-only pure slot conversion.
- `crates/mh3g-save-convert/src/profile.rs`: accepted `user1`/`user2`/`user3`
  and `system` basenames and the source/Cemu byte-size profiles.
- `crates/mh3g-save-convert/src/transaction.rs`: atomic core/system install,
  backup, manifest, history, lock, and rollback boundaries.
- `crates/mh3g-save-convert/src/cec.rs`: inbox-only experimental CEC import,
  `cec` target validation, and CEC backup/manifest behavior.
- `crates/mh3g-save-convert/tests/cli.rs` and
  `crates/mh3g-save-convert/src/converter.rs` tests: dry-run non-write,
  cross-slot rejection, eight-component staging, CEC outbox rejection, and
  offline-hunter/card-anchor regression coverage.
