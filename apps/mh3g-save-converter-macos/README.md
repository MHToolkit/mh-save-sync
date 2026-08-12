# MH3G Save Converter for macOS

[简体中文](README.zh-CN.md)

This is the native SwiftUI front end for the `mh3g-save-convert` Rust CLI. It
invokes the bundled sidecar with an argv array and presents its JSON reports;
no save conversion is reimplemented in Swift.

## Two operation modes

- **New conversion**: convert an original 3DS `user#` into the same-named Cemu
  `user#`.
- **Repair converted save**: use the original 3DS `user#` and the current,
  continued-play Cemu `user#` as separate read inputs, then write the repaired
  result to an independently selected same-slot output. Only fields that still
  retain a 0.0.3 through 0.0.6 conversion result are repaired.

Repair mode is domain-scoped. Core, guild cards, quests, shared `system`, and
experimental CEC each have their own Dry Run, write authorization, manifest,
and rollback. A failed or incomplete optional domain never blocks or revokes an
independently authorized domain.

Each domain displays separate original 3DS, read-only current Wii U/Cemu, and
output controls. Path values never cascade between those controls. Core accepts
an exact `user#` file or its direct parent. `system` and CEC use exact files.
Cards and quests share one ExtData source/current/output directory triplet
because the files are siblings, but they retain separate actions and manifests.
The selected output group must already be initialized and complete: all four
`card*` files or all four `quest*` files. Quest repair preserves the current
Wii U bytes exactly. Ambiguous historical detection requires an explicit
0.0.3-0.0.6 revision followed by another Dry Run; that explicit selection is
reused for core, cards, and quests from the same historical conversion.

The core picker accepts an exact `user1`, `user2`, or `user3` file or its
direct parent. It does not recursively scan an SD card or MLC and does not open
ZIP, 7z, or RAR archives. A directory resolves only to the directly contained
same-named selected slot.

Every write is bound to its domain's immediately preceding Dry Run SHA-256
values. Core uses `.mh3g-compatibility-repair-<UUID>.json` and
`rollback-repair`; cards/quests use `rollback-extras`; system uses `rollback`;
CEC uses `rollback-cec`. Quit Nemessix, Azahar, and Cemu before any write or
rollback.

The optional housekeeper gallery/movie repair never replaces shared `system`
from the 3DS file alone. Select the 3DS source, current initialized Cemu
authority, and an independent output. The converter unions only the known
gallery/movie flags and preserves every other current Cemu byte, including data
shared by the other character slots.

## Updates

The About & Updates section resolves the latest stable tag through the official
`MHToolkit/mh-save-sync` GitHub release page and reads its official Atom release
feed. This path does not consume the shared anonymous GitHub API quota; the
Release API remains a fallback. The first launch on each local calendar day
makes at most one silent attempt. A blocked or unavailable GitHub connection
never blocks the window or any local conversion; manual checks show the error
and may be retried. When a newer release exists, the app shows its title,
publication date, release notes, and official release link.

## Local verification

```bash
swift test
cd ../..
bash scripts/build-mh3g-save-converter-macos-app.sh
bash scripts/mh3g-save-converter-macos-smoke.sh
```

These commands use test fixtures and do not launch Cemu or write a real MLC.
See the [exact file contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) for
CLI scope and examples.
