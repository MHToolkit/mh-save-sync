# MH3G Save Converter for macOS

[简体中文](README.zh-CN.md)

This is the native SwiftUI front end for the `mh3g-save-convert` Rust CLI. It
invokes the bundled sidecar with an argv array and presents its JSON reports;
no save conversion is reimplemented in Swift.

## Two operation modes

- **New conversion**: convert an original 3DS `user#` into the same-named Cemu
  `user#`.
- **Repair converted save**: merge the original 3DS `user#` with the current
  Cemu `user#` after continued play, repairing only fields that still retain a
  0.0.3 through 0.0.6 conversion result.

Repair mode may also select the complete 3DS ExtData `user` directory for
guild-card repair. Current Cemu `card1`, `card2`, `card3`, `cardbox`, and
`quest1` through `quest4` are resolved beside the selected current `user#`;
quest files are validated and preserved, not rewritten by compatibility
repair. Ambiguous detection requires an explicit historical version followed
by another Dry Run.

The core picker accepts an exact `user1`, `user2`, or `user3` file or its
direct parent. It does not recursively scan an SD card or MLC and does not open
ZIP, 7z, or RAR archives. A directory resolves only to the directly contained
same-named selected slot.

Every write is bound to the immediately preceding Dry Run's SHA-256 values.
Normal conversion uses a single-file manifest; compatibility repair uses
`.mh3g-compatibility-repair-<UUID>.json` and `rollback-repair`. Quit Nemessix,
Azahar, and Cemu before any write or rollback.

The optional housekeeper gallery/movie migration never creates or replaces a
shared `system` from the 3DS file alone. Select both the 3DS source `system` and
an existing initialized Cemu `system`; the converter unions only the known
gallery/movie flags and preserves every other Cemu byte, including data shared
by the other character slots.

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
