# MH Save Sync

[English](README.md) | [简体中文](README.zh-CN.md)

Cross-platform, multi-emulator save synchronization for macOS and Android with
end-to-end encrypted snapshots and a self-hosted service.

This repository is in **research/alpha**. It must not be treated as a stable
backup product until the data-integrity gates in `docs/ROADMAP.md` pass.

## Safety invariants

- Local emulator saves remain in their original format and location.
- File watchers mark saves dirty; they never upload directly.
- Remote data is never written into a running emulator's save directory.
- Concurrent histories become conflict branches, never silent last-write-wins.
- A restore snapshots the current state before replacing anything.
- The service never receives recovery secrets or plaintext save contents.

## Public GitHub Actions CI and releases

This public repository uses **standard GitHub-hosted runners** for normal CI
and MH3G converter validation. Pull requests and `main` validate tests and
package-build success without producing distributable archives. Signed-off
public distribution files are created only by the tag-driven release workflow
and uploaded as GitHub Release assets with individual SHA-256 files. Short-lived
GitHub Actions artifacts, when present, are diagnostic handoffs only
(`retention-days: 3`).

| Workflow | Trigger | Hosted validation or output |
| --- | --- | --- |
| `ci.yml` | pull request, `main`, manual | Rust, Android, macOS smoke and compose gates |
| `mh3g-converter-windows.yml` | converter changes, manual | Windows x64 tests plus WinUI/sidecar publish validation; no distributable ZIP/portable/setup upload |
| `mh3g-converter-macos.yml` | converter changes, manual | Apple Silicon SwiftUI app, bundled CLI and validation-only package build; no release archive upload |
| `mh3g-converter-auto-patch-release.yml` | merged PR into `main` | atomically increments MH3G Converter patch version, commits it, creates `v*`, then dispatches the verified release workflow |
| `mh3g-converter-release.yml` | `v*` tag, manual | repeat converter gates, package Windows/macOS, verify SHA-256, then publish Release assets |

Release publishing is tag-only: the final job checks `refs/tags/v*` and alone
gets `contents: write`. Pull-request jobs have read-only repository access and
never receive release/signing secrets. CI proves file-format, CLI, package-build and
native-app build contracts. PR/main validation deliberately stops before
creating player-facing archives; it never launches Cemu, writes a real MLC, or
claims game-runtime verification.

After any PR is merged into `main`, `mh3g-converter-auto-patch-release.yml`
serializes the next patch release (`0.0.7` -> `0.0.8`), commits the manifest
version, creates an annotated `v0.0.8` tag, and explicitly dispatches
`mh3g-converter-release.yml` at that tag. Explicit dispatch is intentional:
tags created by the repository `GITHUB_TOKEN` do not reliably emit a second
push workflow event. The release workflow still performs all gates and creates
the GitHub Release only after Windows/macOS packages and SHA-256 verification
succeed.

## Japanese MH3G 3DS -> Cemu conversion (offline)

`mh3g-save-convert` migrates **one Japanese MH3G 3DS slot** to the matching
Japanese MH3G HD Cemu slot. It is local-only and one-way: it never uploads a
save, never modifies the source file, does not support another region, and does
not convert Cemu back to 3DS. It preserves bytes outside documented conversion
ranges, but preserved bytes are not proof that every in-game field has the same
meaning on both platforms.

### Native macOS and Windows workbenches (development)

`apps/mh3g-save-converter-macos` is a separate foreground SwiftUI app, and
`apps/mh3g-save-converter-windows` is its WinUI companion; neither is the
existing MH Save Sync menu-bar client. They use the bundled
`mh3g-save-convert` executable through argv arrays and its JSON reports. The
apps do not implement byte conversion, backup, manifest, process checks, or
rollback themselves. The macOS window appears in the Dock and Cmd-Tab, follows
the system language by default, and can switch between Simplified Chinese and
English in Settings.

Both native workbenches expose **About & Updates**. The first launch on each
local calendar day makes at most one non-blocking request to the official
`MHToolkit/mh-save-sync` latest-release endpoint; users may also check
manually. An unavailable GitHub connection never blocks conversion. A newer
release dialog shows its version, publication date, release notes, and official
link. No save bytes or selected paths are sent with this request.

The workbenches provide four **guided but non-blocking** stages: input and
inspection, optional shared data, Dry Run, then write or rollback. Completing
one stage reveals the recommended next action (for example, inspect then
optional data), while still allowing a player to revisit a prior stage or skip
optional groups. The guidance is not a separate conversion operation.

For a core slot, the GUI may select either one exact `user1`, `user2`, or
`user3` file or its direct parent directory, then resolves only the selected
slot's direct child. A target selection may be an existing same-name Cemu
`user#` file or an explicit export directory; an export directory resolves to
`<directory>/user#` without creating it during selection. The GUI never
recursively scans, searches an SD card, discovers an MLC root, or accepts an
archive. These directory conveniences are **GUI-only**: the CLI still requires
the exact source and output files documented below.

The optional ExtData picker accepts either the precise `user` directory or its
normal direct `00000481` parent and resolves only that parent's `user` child.
CEC is an independent, collapsed experimental page and is never needed for the
normal guild-card/offline-partner group. A core write remains disabled until its
current Dry Run fingerprint matches the selected source, target, and component
scope. For an existing target, that includes its target SHA-256. For a new
export, the GUI records that the target was absent and writes with
`--expected-target-absent`; if that path appears after Dry Run, the write is
refused rather than overwriting it.

On an arm64 macOS development host, build and exercise only synthetic fixtures:

```bash
bash scripts/build-mh3g-save-converter-macos-app.sh
bash scripts/mh3g-save-converter-macos-smoke.sh
bash scripts/package-mh3g-save-converter-macos.sh
```

The smoke test creates a temporary zero-content fixture and always verifies
inspect, dry-run, bundled diagnostics, and source-hash preservation. When no
emulator is running, it also verifies transactional write and manifest-bound
rollback. When Nemessix, Azahar, or Cemu is already running, it instead proves
the CLI refuses the synthetic write and leaves the temporary target absent. It
never launches Cemu or opens a real MLC.

For a Windows 10 1809+/Windows 11 x64 local WinUI package, do not split the
build into IDE/Qoder-specific manual commands. From the repository root, the
single package script preflights .NET 8, Rust MSVC, and Visual Studio Build
Tools/Windows SDK, then builds and self-checks three WinUI + Rust-sidecar Windows x64 formats: the ZIP, per-user installer EXE, and direct-launch portable EXE:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1
```

Append `-Bootstrap` only for a first machine with missing prerequisites; it uses
`winget` to install missing components and can request administrator approval.
If WinGet has a stale Rustup registration but the current user lacks
`rustup.exe`, the same bootstrap repairs it in place and falls back to a
official HTTPS bootstrapper with a SHA-256 sidecar integrity check, without
deleting `.cargo` or `.rustup`.
Normal runs never clear NuGet/Cargo caches. They also reuse a valid private
.NET 8 SDK from an earlier package version at
`%LOCALAPPDATA%\MH3GSaveConverter\BuildTools\dotnet8\dotnet.exe`, so an SDK
installed with `-NoPath` is not downloaded again. Artifacts, SHA-256 files, and
a failure transcript are written beneath `artifacts\`:
`mh3g-save-convert-windows-x64.zip`, `MH3GSaveConverter-Setup-x64.exe`, and
`MH3GSaveConverter-Portable-x64.exe` each receive their own `.sha256`. The
single-file portable UI extracts its bundled runtime and Rust sidecar to a
per-user cache on first launch. Return the first failed command block in
`mh3g-save-convert-windows-build-transcript.txt` from a tester. See the complete Windows contract and optional switches in
[`apps/mh3g-save-converter-windows/README.md`](apps/mh3g-save-converter-windows/README.md).

> **Read this first:** the current CLI does **not** read ZIP, 7z, or RAR
> archives directly, and it does **not** search an arbitrary save directory for
> a plausible file. Fully extract an archive to a normal local directory, then
> give the command the exact file or exact directory shown below. The core
> directory shortcuts described for the GUI do **not** apply to CLI commands.
> Do not run a program from a QQ/browser archive preview. Quote every path that
> contains a space.

Before any `--write`, `rollback`, `rollback-extras`, or `rollback-cec`, fully quit Nemessix,
Azahar, and Cemu and wait for their processes to stop. `inspect`,
`inspect-progress`, `inspect-events`, `inspect-cec`, and every `--dry-run` are
read-only.

### Select the right extracted input

The Japanese MH3G title savedata and its shared data live in three different
places. The ExtData root is `00000481`, but `convert-extras` needs its `user`
child. CEC is a system NAND mailbox, not SD-card ExtData.

| Data group | Give this exact input to the CLI | Do **not** give it | Required? | Purpose / affected files |
| --- | --- | --- | --- | --- |
| Core slot | One explicit `user1`, `user2`, or `user3` file under `title/00040000/00048100/data/00000001/` | The title directory, all slots, ExtData, a ZIP | Yes: choose one slot | Character, story/progress, farm, fleet, local offline-hunter data; writes only the named same-number Cemu `user#` target |
| Shared system | One explicit 3DS `system` file in the same title savedata directory **and** one existing initialized Cemu `system` target | The whole title directory, a ZIP, or a missing/new Cemu target | Optional | Unions the housekeeper gallery/movie unlock flags into the existing Cemu `system`; all other Wii U shared bytes are preserved |
| Shared ExtData | The complete `extdata/00000000/00000481/user/` directory containing `card1`, `card2`, `card3`, `cardbox`, `quest1`, `quest2`, `quest3`, `quest4` directly inside | The `00000481` parent, `boss/`, a partial set, a ZIP | Optional | Converts all eight files into a new staging directory; a separate guarded `install-extras` transaction can install complete `guild-cards`, `quests`, or both into an initialized Cemu target |
| StreetPass / Hunter Search CEC | The exact `CEC/00048100/` directory containing `InBox___` | SD-card ExtData, the `InBox___` child alone, a ZIP | Optional and experimental | Reads received raw StreetPass records and can write only Cemu `cec` |

For Nemessix, replace `<ID0>` and `<ID1>` with the two 32-hexadecimal
directory names below `sdmc/Nintendo 3DS/`; zero-filled IDs are only a common
local emulator example:

```text
3DS title savedata
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/title/00040000/00048100/data/00000001/
    user1  user2  user3  system

3DS MH3G shared ExtData
  .../sdmc/Nintendo 3DS/<ID0>/<ID1>/extdata/00000000/00000481/
    user/                         <- pass this directory to convert-extras
      card1 card2 card3 cardbox quest1 quest2 quest3 quest4
    boss/ icon metadata            <- not converter inputs

3DS system StreetPass CEC mailbox
  .../nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100/
    InBox___/BoxInfo_____ and InBox___/_*  <- received messages
    OutBox__/...                           <- local outgoing broadcast
```

If an extracted folder contains `user2`, pass that **file** to the CLI for a
core slot conversion. In a native workbench, selecting that folder with
`user2` chosen resolves only its direct `user2` child. If it contains `card1`
through `quest4`, pass that folder only to `convert-extras`. If it contains an
extra wrapper directory, enter it first; the expected filenames must be
immediate children of the CLI path or the narrow GUI selection described above.

If the 3DS `system` file is omitted, a core `user#` conversion cannot migrate
the housekeeper's gallery/movie unlock history. `system` is shared across all
three character slots and also contains settings unrelated to the selected
slot. Therefore `convert-system` requires both the 3DS source and an existing,
initialized Cemu target. It bitwise-unions only the verified gallery/movie
flag range (Cemu file offsets `0x68..0x77`) and preserves every other target
byte. It refuses a new/missing target instead of replacing all shared data.

### Legacy Wii U save-editor caveat

Talisman skill points are signed one-byte values in the equipment record. Some
older Wii U editors read them as unsigned bytes, so a legitimate `-5` appears
as `251`. The converter preserves the raw signed byte and converts only the
record fields whose byte order actually differs. Some legacy armor writers
also rebuild an armor header without preserving its RGB bytes; editing armor
with such a tool can turn its pigment black even when the converted talisman is
valid. These editor behaviors must not be "fixed" by clamping or rewriting
otherwise valid converted equipment records.

### Before you write: paths, inspection, and dry-run

The examples below run from this repository after Rust is installed. Define a
shell array once; when using a packaged binary, replace it with
`CLI=("/path/to/mh3g-save-convert")`.

```bash
CLI=(cargo run --quiet -p mh3g-save-convert --)

# Replace the two IDs and the Cemu user directory with your own extracted paths.
N3DS_ROOT="$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/<ID0>/<ID1>"
SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/user2"
SYSTEM_SOURCE="$N3DS_ROOT/title/00040000/00048100/data/00000001/system"
EXTRAS_SOURCE="$N3DS_ROOT/extdata/00000000/00000481/user"
CEC_SOURCE="$HOME/Library/Application Support/Nemessix/nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100"

# Example only: choose the existing Cemu account directory that contains user# files.
CEMU_DIR="$HOME/Library/Application Support/Cemu/mlc01/usr/save/00050000/10104D00/user/80000001"
TARGET="$CEMU_DIR/user2"
CEMU_CEC="$CEMU_DIR/cec"

"${CLI[@]}" --help
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET"
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET"
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

`inspect` takes exactly one `user#` or `system` file and writes nothing.
`inspect-progress` takes a source slot, optional `--target <Cemu-user#>`, and
optional `--quest-id <0..65535>` to restrict output. `inspect-events` takes a
source slot, optional `--target <Cemu-user#>`, and optional `--all` to include
unset event coordinates. The progress decoder maps quest IDs, including the 16
completion words at payload offset `0x6E5C`; the event decoder covers 58 simple
event words at `0x62AE` and the categorized table at `0x668C`.

`convert` accepts one source `user#` file and `--output <same-name-user#>`.
`user2` can only write a target named `user2`; it cannot overwrite `user1` or
an arbitrary renamed file. With no `--write`, conversion remains a dry-run;
pass `--dry-run` explicitly in scripts to make that intention visible. `--write`
and `--dry-run` conflict.

For GUI and automation clients, `convert` exposes guarded write preconditions:
`--expected-source-sha256` plus exactly one target condition, either
`--expected-target-sha256` or `--expected-target-absent`. `convert-system`
always requires an existing Cemu baseline and therefore requires both
`--expected-source-sha256` and `--expected-target-sha256` when writing. These
arguments are accepted only together with `--write`. Take hash values only from
`hashes.source` and `hashes.target_before` in the JSON emitted by the **same**
immediately preceding Dry Run for the same source and output paths. This makes
the write fail closed if an existing source or target changed; the target hash
is rechecked after the per-slot installation lock is acquired. Do not reuse an
older report or calculate replacement values separately.

`hashes.target_before` is present only when the target file already exists. If
it is absent, do not invent a sentinel hash or pass
`--expected-target-sha256`. For a guarded new export, pass the Dry Run source
hash with `--expected-target-absent` instead. The transaction checks again
after acquiring its lock and refuses the write if the new target appeared in
the meantime. The two target conditions are mutually exclusive.

### Complete command reference

All commands below use the `CLI` array above. Replace it with a packaged binary
if you are not building from source.

#### `inspect` — read one file

```text
mh3g-save-convert inspect <SOURCE>
```

`<SOURCE>` is one recognized Japanese 3DS or Cemu `user1`/`user2`/`user3` or
`system` file. It validates the 3DS `0x2B` profile or the Cemu container,
reports profile/size/hash information, and writes nothing. This is also the
readback check for a converted output:

```bash
"${CLI[@]}" inspect "$SOURCE"
"${CLI[@]}" inspect "$SYSTEM_SOURCE"
```

#### `inspect-progress` — read quest completion

```text
mh3g-save-convert inspect-progress [--target <TARGET>] [--quest-id <QUEST_ID>] <SOURCE>
```

`<SOURCE>` is one 3DS `user#`; `--target` is an optional same-slot Cemu
`user#`; `--quest-id` filters to one numeric quest ID. It writes nothing:

```bash
"${CLI[@]}" inspect-progress "$SOURCE" --target "$TARGET" --quest-id 201
```

#### `inspect-events` — read story/event flags

```text
mh3g-save-convert inspect-events [--target <TARGET>] [--all] <SOURCE>
```

`<SOURCE>` and optional `--target` are the same kind of files as above.
`--all` adds unset coordinates; without it the report focuses on active values.
It writes nothing:

```bash
"${CLI[@]}" inspect-events "$SOURCE" --target "$TARGET" --all
```

#### `convert` — convert one character slot

```text
mh3g-save-convert convert [--dry-run | --write [--expected-source-sha256 <SHA256>] [--expected-target-sha256 <SHA256> | --expected-target-absent]] --output <OUTPUT> <SOURCE>
```

`<SOURCE>` and `<OUTPUT>` must have the same `user#` basename. Read-only use:

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run
```

With all emulators stopped, install atomically:

```bash
"${CLI[@]}" convert "$SOURCE" --output "$TARGET" --write
```

For a GUI or automation write to an existing target, preserve one Dry Run JSON
document and pass its hashes as argv values. This Bash example needs `jq`; it
does not use `eval` or reconstruct a shell command from JSON:

```bash
set -euo pipefail

DRY_RUN_JSON=$("${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run)
SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$DRY_RUN_JSON")
TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$DRY_RUN_JSON")

"${CLI[@]}" convert "$SOURCE" --output "$TARGET" \
  --expected-source-sha256 "$SOURCE_SHA256" \
  --expected-target-sha256 "$TARGET_SHA256" \
  --write
```

`jq -e` stops this **existing-target** guarded path if the target was absent or
the report did not contain both hashes. The two `--expected-...` flags must not
be supplied to a Dry Run or without `--write`.

For a guarded first export, the target must be absent in that Dry Run. Bind the
source hash and absence condition instead of fabricating a target hash:

```bash
EXPORT_DIR="$HOME/Downloads/mh3g-cemu-export"
TARGET="$EXPORT_DIR/user2"
NEW_DRY_RUN_JSON=$("${CLI[@]}" convert "$SOURCE" --output "$TARGET" --dry-run)
NEW_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$NEW_DRY_RUN_JSON")

"${CLI[@]}" convert "$SOURCE" --output "$TARGET" \
  --expected-source-sha256 "$NEW_SOURCE_SHA256" \
  --expected-target-absent \
  --write
```

Do not create `"$TARGET"` between the Dry Run and write. If another process
creates it, the transaction intentionally refuses the write rather than
overwriting a newly appeared save.

If a target existed, `--write` creates
`.user2.mh3g-backup-<previous-sha256>` beside it, plus
`.user2.mh3g-install.json`; repeated installs may create
`.user2.mh3g-install-history-<sha256>.json`. Keep the manifest until manual
Cemu validation succeeds.

#### `repair-converted` — independently repair the core slot

```text
mh3g-save-convert repair-converted <original-3DS-user#> --current <current-Cemu-user#> \
  [--output <repaired-Cemu-user#>] \
  [--source-extdata-dir <legacy-coupled-3DS-ExtData-user>] [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] \
  [--dry-run | --write --expected-source-set-sha256 <SHA256> \
    --expected-current-set-sha256 <SHA256> --expected-output-set-sha256 <SHA256> \
    --expected-preview-sha256 <SHA256>]
```

Use this command when a save was converted with 0.0.3 through 0.0.6 and then
played further on Wii U/Cemu. It does not replace the current save with the old
3DS state. For each known historical conversion field it compares the older
expected value, the current value, and the current converter's expected value. Only fields
still equal to the old result are repaired; complete fields changed on Wii U
are preserved and reported as conflicts. Current HR, equipment, materials,
storage, quest progress, farm, fleet, and other later gameplay therefore remain
owned by the current Cemu save.

The three paths have deliberately separate roles: the original 3DS slot is a
read-only conversion source, `--current` is the read-only Wii U/Cemu state that
owns later gameplay progress, and `--output` is the only core slot that may be
written. All three must be exact, same-numbered `user1`, `user2`, or `user3`
paths. Omitting `--output` retains the CLI's legacy in-place behavior and uses
`--current` as the destination; the native macOS and Windows workbenches never
hide this coupling and always display/pass a separate output selection.

The native macOS and Windows workbenches may accept a core file or its direct
parent directory, but they still resolve and pass one exact file to the CLI.
They use `repair-converted` for the core slot only. The old
`--source-extdata-dir` option remains available to existing scripts, but it is
no longer used by either native UI; new repairs use the independent domain
commands below. `system`, `cec`, `phrase1` through `phrase3`, and unknown files
are not read or written by a core-only run.

Start with a read-only preview:

```bash
# SOURCE, CURRENT, and OUTPUT are three distinct same-slot user# paths.
REPAIR_JSON=$("${CLI[@]}" repair-converted "$SOURCE" --current "$CURRENT" --output "$OUTPUT" \
  --dry-run)
```

If the top-level `detection.confidence` is `ambiguous`, do not write. Confirm
the original version from its `candidates`, then repeat Dry Run with
`--from-version`. Every selected component shares this one revision decision;
the converter never repairs `user#` and `card*` as different historical
releases. Detection cannot read an embedded converter version because older
releases did not store a trustworthy marker. A write must reuse all four
authorization hashes from that final Dry Run:

```bash
SOURCE_SET_SHA256=$(jq -er '.source_set_sha256' <<<"$REPAIR_JSON")
CURRENT_SET_SHA256=$(jq -er '.current_set_sha256' <<<"$REPAIR_JSON")
OUTPUT_SET_SHA256=$(jq -er '.output_set_sha256' <<<"$REPAIR_JSON")
PREVIEW_SHA256=$(jq -er '.preview_sha256' <<<"$REPAIR_JSON")

"${CLI[@]}" repair-converted "$SOURCE" --current "$CURRENT" --output "$OUTPUT" \
  --expected-source-set-sha256 "$SOURCE_SET_SHA256" \
  --expected-current-set-sha256 "$CURRENT_SET_SHA256" \
  --expected-output-set-sha256 "$OUTPUT_SET_SHA256" \
  --expected-preview-sha256 "$PREVIEW_SHA256" \
  --write
```

If Dry Run used `--from-version`, pass the same value to the write. Any source,
current reference, output state, or preview change between the two steps fails closed. A
successful core write returns a coordinator manifest named
`.mh3g-compatibility-repair-<UUID>.json`. Roll it back with:

```bash
"${CLI[@]}" rollback-repair --manifest "$COMPATIBILITY_MANIFEST"
```

The deprecated `--source-extdata-dir` path is documented only for older
automation. A `no-changes` report means the core needed no write, so no empty
coordinator manifest is made.

#### Independent repair domains

Repair mode exposes five separate transactions. Original 3DS, current
Wii U/Cemu, and output are distinct path roles; selecting one never fills or
changes another. Every row has its own Dry Run, write authorization, manifest,
and rollback:

| Domain | Command | Required source/current/output |
| --- | --- | --- |
| Core | `repair-converted` | Three same-named `user#` paths |
| Guild cards | `repair-extras --group guild-cards` | Three ExtData directories; output already contains `card1`, `card2`, `card3`, `cardbox` |
| Quests | `repair-extras --group quests` | Three ExtData directories; output already contains `quest1` through `quest4` |
| Gallery/movie `system` | `repair-system` | Exact 3DS `system`, exact current Cemu `system`, exact output `system` |
| Experimental CEC | `repair-cec` | Exact 3DS CEC mailbox directory, exact current Cemu `cec`, exact output `cec` |

Cards and quests physically share one ExtData directory picker triplet in the
native UIs, but each group is still authorized and rolled back separately.
Quest repair validates the original group and preserves the current Wii U
bytes exactly. `system` unions only the verified gallery/movie flags into the
current authority. CEC remains opt-in experimental. An incomplete optional
domain never blocks core repair or another complete domain.

```text
mh3g-save-convert repair-extras --group <guild-cards|quests> \
  --source-dir <3DS-ExtData-user> --current-dir <current-Cemu-dir> \
  --output-dir <initialized-output-Cemu-dir> [--dry-run | --write ...]

mh3g-save-convert repair-system <3DS-system> --current <current-Cemu-system> \
  --output <output-Cemu-system> [--dry-run | --write ...]

mh3g-save-convert repair-cec --source-dir <3DS-CEC-mailbox> \
  --current <current-Cemu-cec> --output <output-Cemu-cec> \
  [--dry-run | --write --experimental ...]
```

Use `rollback-extras`, `rollback`, and `rollback-cec` with the manifest returned
by the matching domain. See the exact file contract for all expected hash
arguments and fail-closed completeness rules.

#### `convert-system` — safely merge shared gallery/movie flags

```text
mh3g-save-convert convert-system [--dry-run | --write --expected-source-sha256 <SHA256> --expected-target-sha256 <SHA256>] --output <EXISTING_CEMU_SYSTEM> <3DS_SYSTEM>
```

Use an explicit 3DS `system` source and an existing initialized Cemu `system`
target; it never reads a `user#` or ExtData:

```bash
"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$CEMU_DIR/system" --dry-run
```

The command does not replace the complete shared file. It preserves the Cemu
header, settings, and unknown/shared-slot records, and unions only the verified
gallery/movie bitset at Cemu offsets `0x68..0x77`. The same transactional
backup/manifest pattern applies, using `.system...` names. `--write` and
`--dry-run` conflict. A write always requires the two hashes emitted by that
immediately preceding `convert-system` Dry Run, not hashes from a slot
conversion:

```bash
SYSTEM_TARGET="$CEMU_DIR/system"
SYSTEM_DRY_RUN_JSON=$("${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" --dry-run)
SYSTEM_SOURCE_SHA256=$(jq -er '.hashes.source' <<<"$SYSTEM_DRY_RUN_JSON")
SYSTEM_TARGET_SHA256=$(jq -er '.hashes.target_before' <<<"$SYSTEM_DRY_RUN_JSON")

"${CLI[@]}" convert-system "$SYSTEM_SOURCE" --output "$SYSTEM_TARGET" \
  --expected-source-sha256 "$SYSTEM_SOURCE_SHA256" \
  --expected-target-sha256 "$SYSTEM_TARGET_SHA256" \
  --write
```

There is intentionally no new-`system` export mode. Start MH3G HD once so it
creates a valid Wii U/Cemu `system`, stop the emulator, then select that file
as the merge baseline. This protects settings and records shared by other
character slots.

#### `convert-extras` — stage shared ExtData

```text
mh3g-save-convert convert-extras [--dry-run | --write] [--reset-guild-cards] \
  --source-dir <EXTDATA-USER-DIR> --output-dir <NEW-STAGING-DIR>
```

`--source-dir` must be the complete `.../extdata/00000000/00000481/user/`
directory; all eight files are required even if you later install only card
files. `--output-dir` is a new staging directory. Existing named component
files are refused, so this command never overwrites a Cemu save:

```bash
EXTRAS_OUTPUT="$HOME/Desktop/mh3g-cemu-extras"
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --dry-run
"${CLI[@]}" convert-extras --source-dir "$EXTRAS_SOURCE" --output-dir "$EXTRAS_OUTPUT" --write
```

`quest1`–`quest4` receive the Cemu container. `card1`–`card3` and `cardbox`
receive the recovered cross-platform field mapping before the wrapper is
written. `--reset-guild-cards` is an explicit destructive recovery switch: it
generates empty native-Cemu `card*` files and discards local/received card data.
Do not use it for normal migration. Do not manually copy individual generated
files into Cemu: use the guarded complete-group installer below.

#### `install-extras` — transactionally install staged ExtData groups

```text
mh3g-save-convert install-extras [--dry-run | --write] \
  --staging-dir <STAGING-DIR> --target-dir <INITIALIZED-CEMU-SAVE-DIR> \
  --groups <guild-cards,quests>
```

This is intentionally separate from `convert-extras`. `--staging-dir` must
contain all eight generated files, while `--groups` chooses only whole groups:
`guild-cards` means `card1`, `card2`, `card3`, and `cardbox`; `quests` means
`quest1` through `quest4`. The target must be an initialized MH3G Cemu save
directory containing the selected named components. A write creates a
manifest-bound recovery transaction and retains the previous target bytes; it
never installs one `card#` or `quest#` file by itself.

On Windows, complete-group writes use `ReplaceFileW`, manifest-bound backups,
and a durable recovery journal. The same complete-group, Dry Run hash, and
rollback requirements apply; individual `card#` or `quest#` installation is
still refused.

Run the installation Dry Run immediately before writing, and bind both reported
set hashes to the write:

```bash
EXTRAS_INSTALL_DRY_RUN_JSON=$("${CLI[@]}" install-extras \
  --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
  --groups guild-cards,quests --dry-run)
EXTRAS_STAGING_SHA256=$(jq -er '.staging_set_sha256' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")
EXTRAS_TARGET_SHA256=$(jq -er '.target_set_sha256_before' <<<"$EXTRAS_INSTALL_DRY_RUN_JSON")

"${CLI[@]}" install-extras \
  --staging-dir "$EXTRAS_OUTPUT" --target-dir "$CEMU_DIR" \
  --groups guild-cards,quests \
  --expected-staging-set-sha256 "$EXTRAS_STAGING_SHA256" \
  --expected-target-set-sha256 "$EXTRAS_TARGET_SHA256" \
  --write
```

#### `inspect-cec` — read StreetPass/Hunter Search mailbox

```text
mh3g-save-convert inspect-cec --source-dir <CEC-DIR> [--target <CEMU-CEC>] \
  [--source-slot <USER-SLOT>]
```

`--source-dir` is the CEC `.../CEC/00048100/` directory, not ExtData.
`--target` optionally reads a Cemu `cec` file. `--source-slot` optionally reads
one `user#` only to locate its guild-card anchor. The command writes nothing:

```bash
"${CLI[@]}" inspect-cec --source-dir "$CEC_SOURCE" --source-slot "$SOURCE" --target "$CEMU_CEC"
```

#### `convert-cec` — experimental received-message import

```text
mh3g-save-convert convert-cec --source-dir <CEC-DIR> --target <CEMU-CEC> \
  [--slot <SLOT>] --dry-run

mh3g-save-convert convert-cec --source-dir <CEC-DIR> --target <CEMU-CEC> \
  [--slot <SLOT>] --write --experimental \
  --expected-source-record-set-sha256 <SHA-256> \
  --expected-target-sha256 <SHA-256>
```

CEC is not the main save and not the durable guild-card store. `InBox___/_*`
are raw received messages; `OutBox__/_*` are the local hunter's outgoing
broadcast and are intentionally ignored. `BoxInfo_____` is mailbox metadata.
Only non-empty inbox records are candidates. Existing guild cards and
offline-hall partners use this durable set instead:

```text
matching user# + card1 + card2 + card3 + cardbox
```

An empty CEC inbox is normal even when the durable card list is non-empty.
`convert-cec` is independent and **experimental**: it has file-level evidence,
not a blanket runtime guarantee for every Wii U UI. It writes no `user#`,
`system`, `card*`, or `quest*` file. It uses the first empty Cemu slot by
default; `--slot <SLOT>` chooses the first candidate slot and never overwrites
an existing non-empty record.

The CEC Dry Run reports an order-independent `source_record_set_sha256` and a
`target_sha256_before` (the canonical empty Cemu container if `cec` does not
yet exist). A write requires both values and rechecks them after acquiring the
CEC target lock:

```bash
CEC_DRY_RUN_JSON=$("${CLI[@]}" convert-cec \
  --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --dry-run)
CEC_SOURCE_RECORD_SET_SHA256=$(jq -er '.source_record_set_sha256' <<<"$CEC_DRY_RUN_JSON")
CEC_TARGET_SHA256=$(jq -er '.target_sha256_before' <<<"$CEC_DRY_RUN_JSON")

"${CLI[@]}" convert-cec --source-dir "$CEC_SOURCE" --target "$CEMU_CEC" --slot 0 \
  --expected-source-record-set-sha256 "$CEC_SOURCE_RECORD_SET_SHA256" \
  --expected-target-sha256 "$CEC_TARGET_SHA256" \
  --write --experimental
```

The source 3DS wrapper and observed 8-byte message prefix are not copied. A
successful CEC write creates `.cec.mh3g-backup-<previous-sha256>` when needed
and `.cec.mh3g-install.json` beside the target.

#### `rollback`, `rollback-extras`, and `rollback-cec` — restore only a known transaction

```text
mh3g-save-convert rollback --manifest <MANIFEST>
mh3g-save-convert rollback-extras --manifest <EXTRAS-MANIFEST>
mh3g-save-convert rollback-cec --manifest <MANIFEST>
```

Both commands require an exact converter-generated manifest; they do not accept
a save directory, backup file, or archive. With all emulators stopped:

```bash
"${CLI[@]}" rollback --manifest "$CEMU_DIR/.user2.mh3g-install.json"
"${CLI[@]}" rollback-extras --manifest "$EXTRAS_MANIFEST"
"${CLI[@]}" rollback-cec --manifest "$CEMU_DIR/.cec.mh3g-install.json"
```

Rollback restores only the manifest-bound core target, ExtData group, or CEC
target. It never changes the 3DS source. Preserve the `manifest` path emitted
by each successful write; `install-extras` emits its manifest in the write JSON
because it is not a fixed file name.

### Runtime evidence and packages

The converter recognizes the Japanese `0x2B` profile only. Its static tables
and provenance are in `crates/mh3g-save-convert/data/catalog-provenance.json`;
the detailed data boundary is in
[`docs/adr/0013-mh3g-cross-format-conversion.md`](docs/adr/0013-mh3g-cross-format-conversion.md)
and [the exact MH3G file contract](docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md).

**macOS arm64 (isolated CLI verified 2026-07-26):** the packaged release binary
passed its inner checksum, ZIP checksum, extraction, and extracted `--help`
checks. Two real Japanese sources (`user1` and `user2`) each passed `inspect`,
`inspect-progress`, `inspect-events`, explicit dry-run with no target created,
isolated write, output readback, reinstall/backup/history creation, and
manifest-bound rollback. Both sources remained byte-for-byte unchanged; their
outputs were `0x8A24`, and written hashes matched dry-run hashes. The real
eight-file ExtData directory also passed dry-run with no output directory and
write to a fresh staging directory with all documented output sizes/hashes.
Read-only CEC inspection reported zero received inbox messages and one local
outbox message, so experimental CEC writing was not attempted. Cemu was not
started and no existing MLC was read or written; this is CLI/file validation,
not a new gameplay/runtime claim.

Build and reproduce the same package on an Apple Silicon Mac:

```bash
./scripts/package-mh3g-macos.sh artifacts/mh3g-converter
```

This creates `mh3g-save-convert-macos-arm64.zip`, its `.zip.sha256` sidecar,
and the extracted staging directory with the binary, bilingual README, and
inner binary checksum. The script never reads a save.

**Windows x64:** `.github/workflows/mh3g-converter-windows.yml` builds a native
statically linked `x86_64-pc-windows-msvc` executable, packages its checksum and
launcher, simulates Mark-of-the-Web, and runs synthetic write/rollback evidence.
The current PR's GitHub-hosted workflow result is the package-CI evidence; it
does not prove an individual PC's AppLocker, Smart App Control, antivirus,
archive-preview, or directory-permission policy. Download only a successful
workflow artifact, verify its ZIP SHA-256 sidecar, fully extract it, then use
the included `Run-Converter.ps1`. Retain the complete operation/path line if
Windows reports error 5 (`Access is denied`).


## Office Mac ↔ home Android user flow

Phase 1 alpha now uses a Chinese-first sync workbench so the user can see the
actual target instead of pressing an opaque “sync” button:

1. Run or self-host the server and use the same URL on both devices.
   - macOS: run
     `swift run --package-path apps/macos MHSaveSyncMac --set-server-url <url>`
     once, or set `MH_SAVE_SYNC_SERVER_URL` for one-off CLI sessions.
   - Android: enter the same server address in the app.
   - Current isolated Alpha test API: `http://8.130.112.207:39082`
     (server API only; MinIO/admin ports are not client endpoints).
2. macOS Nemessix before launch: install the local menu-bar app once with
   `./scripts/install-macos-app.sh`, then open `/Applications/MH Save Sync.app`.
   The app is a menu-bar utility: look for `MH 云存档` in the top-right menu bar,
   not in the Dock. First configure `设置服务器地址…`, `选择 Mac Nemessix 存档目录…`
   and `生成恢复密钥文件` (recommended) or `选择恢复密钥文件…`. The menu-bar title changes between
   `MH 云存档 · 设服务器/选目录/选密钥/就绪`, and the menu top lines always show
   `同步路线` and `下一步：...`, so after setting only the server URL it will still
   tell you to pick the save folder and generate/select the recovery-secret file before syncing. Then use `启动前检查` before MH3G,
   `立即上传 Mac 存档到服务器` for manual sync, `我已退出 MH3G：立即对账上传` after
   quitting, `查看云端状态` to see where the save went, and
   `自动同步：退出 Nemessix 后上传` if you want menu-bar exit detection.
   `新手引导：办公室 Mac ↔ 回家 Android` explains the same flow inside the app.
3. Android Nemessix before launch: authorize the Nemessix save folder, keep
   `MH3G / Android Nemessix` enabled, then tap `启动前检查`. Android shows whether
   it is uploading, downloading to the phone cache, waiting for you to exit MH3G,
   or blocked because the server address is missing.
4. If cloud and local versions differ, the app requires an explicit choice:
   `云端覆盖本地（先备份，需停止 Nemessix）` or `本地替换云端（保留云端旧版本）`.
   Both histories are retained; there is no newest-time auto overwrite.
5. If the server is unavailable, continuing local play is explicit. Local queues
   remain intact and upload resumes after the server recovers.

Player-facing Chinese guide: `docs/ux/USER_GUIDE_ZH.md`.
Engineering UX contract: `docs/ux/SYNC_USER_FLOWS.md`.
UI/UX research baseline: `docs/research/UI_UX_PATTERNS.md`.

## Repository map

- `crates/`: shared Rust domain, engine, crypto, adapters, client, server and CLI
- `apps/`: native macOS and Android shells
- `deploy/compose/`: self-hosted PostgreSQL and S3-compatible deployment
- `docs/research/`: evidence-backed research and experiments
- `docs/adr/`: accepted architecture decisions
- `docs/api/openapi-v1.yaml`: REST/OpenAPI v1 contract draft
- `scripts/`: reproducible development, backup and verification tools

## Status

Phase 1 feature branch currently contains:

- evidence-backed cloud-save, crypto/conflict, emulator-matrix, write-timeline and self-hosting research drafts;
- Rust workspace for domain, crypto, engine, adapters, client, server and CLI;
- encrypted fixed-chunk fixture snapshots, DAG conflict preservation, safe restore gating and SQLite WAL metadata tests;
- PostgreSQL + S3/MinIO persistent service with signed device certificates,
  missing-set resumable uploads, checksum fail-closed writes, transactional
  CAS HEAD commits, S3 SHA-256 upload checksums, bucket versioning init and
  readiness checks for committed object references;
- macOS Swift shell smoke, Android SAF/WorkManager/foreground-service shell,
  and generated UniFFI Kotlin/Swift bridge evidence.
- CI supply-chain gates for `cargo deny`, `cargo audit`, dependency review,
  CycloneDX SBOM generation, secret scanning and artifact checksums. The
  self-hosted-compatible PR path currently runs Rust and Android automatically.
  MHToolkit presently exposes one 2c4g `ci-general` runner, so CI cancels stale
  pushes and serializes heavy Rust → Android jobs instead of contending for the
  same host. A separate weekly `ci-canary` runs only lightweight runner/script/
  UX-copy health checks. Self-hosted Compose healthchecks default to 15s/15s/30s
  PostgreSQL/MinIO/server intervals and can be tuned with `MH_SAVE_SYNC_*_HEALTH_*`
  env vars for smaller hosts. `deploy/compose/compose.tls.yaml` adds an optional
  Caddy reverse proxy so production deployments can keep the API on loopback and
  publish only 80/443; macOS and Compose evidence remains recorded in
  `docs/runbooks/PHASE1_VALIDATION.md`.

Still not stable: real macOS↔Android↔second-emulator round trips, polished
export/import UX, upgrade/rollback benchmark, public-trusted TLS endpoint
verification and real-emulator bundle recovery remain open gates in
`docs/ROADMAP.md`. Fixture no-server bundle recovery is covered by
`scripts/offline-bundle-e2e.sh`; the isolated `mh-save-sync-aliyun` deployment,
public Alpha API gate, disaster-recovery gate and optional Caddy TLS reverse
proxy config gate are recorded in `docs/runbooks/PHASE1_VALIDATION.md`.

## Five-minute local demo

```bash
cargo test --workspace
cargo run -p save-cli --bin mh-save -- adapters
cargo run -p save-cli --bin mh-save -- crypto-vector
cargo run -p save-cli --bin mh-save -- crypto-device-fixture
cargo run -p save-cli --bin mh-save -- snapshot-fixture tests/fixtures/generic-save
./scripts/offline-bundle-e2e.sh
./scripts/supply-chain-gate.sh
./scripts/automation-policy-e2e.sh
swift run --package-path apps/macos MHSaveSyncMac
swift run --package-path apps/macos MHSaveSyncMac --set-server-url http://127.0.0.1:18080
./scripts/build-macos-app-bundle.sh
MH_SAVE_SYNC_INSTALL_DIR="$PWD/artifacts/local-apps" ./scripts/install-macos-app.sh
./scripts/macos-shell-e2e.sh
./scripts/macos-config-e2e.sh
./scripts/macos-install-e2e.sh
```

To install the macOS Alpha app for normal double-click usage on this Mac:

```bash
./scripts/install-macos-app.sh
open -a "/Applications/MH Save Sync.app"
```

The app menu can set the server, save directory, recovery-secret file, manual upload,
cloud status, restore and exit-after-upload automation directly. The CLI path below
writes the same persisted config if you prefer scripting:

```bash
"/Applications/MH Save Sync.app/Contents/MacOS/MHSaveSyncMac" \
  --set-server-url http://8.130.112.207:39082
```

macOS shell can call the same Rust CLI pipeline used by Android/CLI demos:

```bash
export MH_SAVE_SYNC_SERVER_URL=http://127.0.0.1:18080
export MH_SAVE_SYNC_CLI="$PWD/target/debug/mh-save"

swift run --package-path apps/macos MHSaveSyncMac --set-server-url "$MH_SAVE_SYNC_SERVER_URL"

swift run --package-path apps/macos MHSaveSyncMac --server-upload \
  --root tests/fixtures/generic-save \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555

swift run --package-path apps/macos MHSaveSyncMac --server-status \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555

swift run --package-path apps/macos MHSaveSyncMac --server-restore \
  --target /tmp/mh-save-sync-restored \
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555 \
  --emulator-state stopped
```

Visible server sync demo (shows where the snapshot went, the logical save ID,
the cloud HEAD and conflict branch count):

```bash
MH_SAVE_SYNC_BIND=127.0.0.1:18080 cargo run -p save-server --bin mh-save-server

cargo run -p save-cli --bin mh-save -- server-upload \\
  --server-url http://127.0.0.1:18080 \\
  --root tests/fixtures/generic-save \\
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555 \\
  --device-id office-mac

cargo run -p save-cli --bin mh-save -- server-status \\
  --server-url http://127.0.0.1:18080 \\
  --secret-hex 5555555555555555555555555555555555555555555555555555555555555555
```

`server-upload` prints Chinese `message_zh`, `server_url`, `sync_target`,
`logical_save_id`, `cloud_head_before`, `cloud_head`, `outcome` and
`conflict_snapshot`, so a manual sync is never a black box. `server-status`
also includes `conflict_diffs` when the client can decrypt the current HEAD and
conflict branches with the recovery secret. Phase 1 exposes an explicit
game-specific parser contract: `mh3g-3ds` currently reports file/byte-level
differences for MH3G/3U 3DS saves and deliberately does **not** claim hunter,
equipment, item or quest semantic merges until a game-specific parser proves
those fields.

Local save-diff smoke:

```bash
cargo run -p save-cli --bin mh-save -- save-diff \\
  --left /tmp/mh-save-left \\
  --right /tmp/mh-save-right \\
  --game-profile mh3g-3ds
```

The reproducible gate is `./scripts/server-sync-e2e.sh`: it uploads an office
snapshot, uploads a home/Android-style divergent branch without a base head,
and verifies the cloud HEAD is preserved while the conflict branch is retained
and user-readable file/byte diff metadata is returned.

Automation policy gate:

```bash
./scripts/automation-policy-e2e.sh
```

This fixes the trigger contract shared by macOS and Android: file-system events
only mark dirty; save-complete, emulator-exit, periodic reconcile and manual
sync may create a stable snapshot candidate; remote restore remains blocked
while an emulator is running.

Android local build/lint, using Android Studio's bundled JBR when macOS has no
system Java:

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug
```

Install/launch smoke for the generated debug APK on exactly one connected ADB
device or emulator:

```bash
./scripts/android-apk-smoke.sh
```

The smoke installs `apps/android/app/build/outputs/apk/debug/app-debug.apk`,
launches `org.mhtoolkit.savesync/.MainActivity`, checks that it becomes the
resumed activity and fails if launch logcat contains an app crash. It does not
exercise SAF or real emulator save access; use it as the quick "can I install
the APK before going home?" gate before the shared-folder sync E2E below.

Android UI copy smoke for the same launched APK:

```bash
./scripts/android-ui-copy-smoke.sh
```

This dumps the actual Android view hierarchy and fails unless the visible app
copy explains `MH 云存档同步`, office Mac ↔ home Android, sync route, no silent
overwrite, server target, Android Nemessix folder authorization, MH3G toggle and
pre-launch check in Chinese.

Android Generic Folder shared-storage smoke with a connected ADB device:

```bash
MH_SAVE_SYNC_SERVER_URL=http://8.130.112.207:39082 \
  ./scripts/android-generic-folder-e2e.sh
```

This verifies the generic user-selected-folder path across macOS, the public
Alpha API and Android `/sdcard` shared storage, including conflict retention and
running-restore fail-closed behavior. It is not a Nemessix/Azahar/Citra runtime
claim; emulator-specific adapters still require emulator-readable restore
evidence.


Offline no-server recovery demo:

```bash
./scripts/offline-bundle-e2e.sh
```

This exports `tests/fixtures/generic-save` to an encrypted `.mhsavebundle`,
restores it to a fresh directory, byte-compares the result, and verifies that
`--emulator-state running` fails closed without writing the target. See
`docs/runbooks/OFFLINE_BUNDLE_RECOVERY.md`.

Self-hosted local demo with external secret files:

```bash
secret_dir="$HOME/Documents/Secrets/mh-save-sync-compose"
mkdir -p "$secret_dir"
openssl rand -hex 32 > "$secret_dir/postgres_password.txt"
printf 'mh-save-sync-local' > "$secret_dir/minio_root_user.txt"
openssl rand -hex 32 > "$secret_dir/minio_root_password.txt"
chmod 600 "$secret_dir"/*.txt
printf 'MH_SAVE_SYNC_SECRETS_DIR="%s"\n' "$secret_dir" \
  > "$HOME/Documents/Secrets/mh-save-sync.env"
chmod 600 "$HOME/Documents/Secrets/mh-save-sync.env"

podman compose --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml up -d --build --wait
curl -fsS http://127.0.0.1:18080/ready
python3 scripts/compose-e2e.py
python3 scripts/compose-resume-e2e.py prepare artifacts/compose-resume-state.json
podman compose --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml restart server
python3 scripts/compose-resume-e2e.py finish artifacts/compose-resume-state.json

# Full persistent backend CLI restore gate. This starts an isolated Compose
# project on free localhost ports, uploads office/home divergent snapshots,
# keeps the cloud HEAD unchanged, restores the cloud HEAD byte-for-byte and
# verifies running-emulator restore fails closed.
CONTAINER_RUNTIME=podman ./scripts/compose-server-sync-e2e.sh
```

`scripts/compose-server-sync-e2e.sh` can also target an already running
persistent server with `MH_SAVE_SYNC_SERVER_URL=...`. When it starts Compose
it checks that the selected runtime daemon is actually usable; if Docker CLI is
installed but the daemon is down, it falls back to Podman instead of failing
mid-test. The lightweight runtime-selection guard is
`./scripts/compose-server-sync-e2e-runtime-test.sh`.

Backup and destructive restore:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/backup.sh

CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
  deploy/compose/scripts/restore.sh "$HOME/Games/Backups/MHSaveSync/<run-id>"
```

Latest local validation evidence is summarized in
`docs/runbooks/PHASE1_VALIDATION.md`.
