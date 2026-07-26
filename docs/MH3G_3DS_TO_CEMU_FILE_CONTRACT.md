# MH3G 3DS to Wii U/Cemu File Contract

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
| Received/local guild-card data and the card side of offline-hall partners | The complete 3DS extdata `user` directory containing all eight files listed below | `convert-extras --source-dir <extdata user> --output-dir <new empty staging dir>` | Only generated files under the staging directory; current CLI has no overwrite/backup installer for these files |
| Downloaded or created quests | The same complete 3DS extdata `user` directory | `convert-extras` | Generated `quest1` through `quest4` in the staging directory |
| StreetPass/Hunter Search cache | Optional 3DS CEC mailbox root with `InBox___/BoxInfo_____` and received message files | `convert-cec --experimental` | Cemu `cec` only, plus its transaction artifacts |

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
It currently has no `--components` or per-file mode.  A future UI may let the
user select which generated files to install, but it must still collect the
complete extdata directory for the current converter and make that selected
installation explicit.

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

### Optional StreetPass/CEC

CEC is neither part of `user#` nor part of `card*`, and it is not required for
the normal guild-card/offline-partner path.  It is an experimental, independent
cache import:

| 3DS input | Cemu output | Write condition |
| --- | --- | --- |
| CEC root's non-empty received MH3G records in `InBox___/_*` | `cec` | `convert-cec --write --experimental` |

`convert-cec` requires `InBox___/BoxInfo_____`; it deliberately ignores
`OutBox__` records because those describe the source hunter's outgoing
transmission.  If the Cemu `cec` file is absent, the tool plans an empty Cemu
container in memory and creates it only with `--write`.  It does not write a
card file or a `user#` file.

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
| `inspect-cec --source-dir ... [--target cec] [--source-slot user#]` | CEC `InBox___` and `OutBox__`; optional `cec` and optional user slot | Nothing | N/A |
| `convert-cec --source-dir ... --target cec` | Received `InBox___` records and the existing `cec`, if any | Nothing | `cec` plus CEC transaction artifacts; requires `--experimental` |
| `rollback` / `rollback-cec` | Their controlled manifest, target, and backup | N/A | Restores or removes only the manifest-bound target and removes its transaction artifacts |

For `convert` and `convert-system`, a successful write uses a same-directory
temporary file and atomic rename.  When an old target exists, it creates a
hash-addressed backup.  The persistent managed files are:

```text
.<user#|system>.mh3g-backup-<previous-sha256>       # only if target existed
.<user#|system>.mh3g-install.json
.<user#|system>.mh3g-install-history-<sha256>.json # possible on reinstall
```

The short-lived `.<user#|system>.mh3g-install.lock` and temporary file are
removed after the transaction.  `convert-extras` deliberately has no target
backup, manifest, or overwrite path: it refuses `--write` if any of its eight
named output files already exists.  Use a fresh staging directory, compare the
reported hashes, and separately snapshot the chosen Cemu files before a UI or
operator installs selected staged components.

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
