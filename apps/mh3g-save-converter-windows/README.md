# MH3G Save Converter for Windows

[简体中文](README.zh-CN.md)

Native Windows companion for the `mh3g-save-convert` Rust CLI. This is an
unpackaged **WinUI 3 / .NET 8** desktop shell for Windows 10 1809+ and Windows
11 x64. It deliberately contains no save parsing, item mapping, or conversion
code.

## Safety boundary

- The UI starts with the system language and persists a `System default`,
  `Simplified Chinese`, or `English` override under
  `%LOCALAPPDATA%\MHToolkit\MH3GSaveConverter\settings.json`.
- It does not search an SD card, MLC, ZIP, 7z, RAR, or a generic save folder.
  The user selects exact paths.
- New conversion exposes `inspect` -> `convert --dry-run` -> final SHA-256
  recheck -> `convert --write`. Repair mode splits core, guild cards, quests,
  shared `system`, and experimental CEC into independent transactions. Every
  domain has separate original 3DS, read-only current Cemu, and output path
  roles plus its own Dry Run, write authorization, manifest, and rollback.
  Path values never cascade between those controls. Core accepts an exact
  `user#` file or its direct parent; `system` and CEC use exact files.
  Cards and quests share one physical ExtData directory triplet but retain
  separate actions/manifests. Each output group must already contain all four
  initialized files. Quest repair preserves current Wii U bytes exactly.
  Ambiguous core/card/quest detection requires choosing 0.0.3 through 0.0.6
  and repeating that domain's Dry Run.
- The UI opens a confirmation dialog before writing. Normal conversion records
  its single-file manifest. Core repair uses
  `.mh3g-compatibility-repair-<UUID>.json`; cards/quests, `system`, and CEC each
  retain their own matching manifest and rollback route. Failure in one domain
  does not revoke another domain's successful Dry Run.
- The process bridge uses `ProcessStartInfo.ArgumentList`, sets
  `UseShellExecute = false`, and parses the CLI's JSON stdout. No shell command
  string is built and no conversion behavior is duplicated in C#.
- Experimental CEC is off by default. It has separate inspect, Dry Run, final
  read-only verification, write, and rollback routes. The immediately preceding
  Dry Run's aggregate source_record_set_sha256 and target_sha256_before are
  bound to its write, so a changed mailbox or cache fails closed. It never turns
  on merely because a player selected the primary slot.
- Shared `system` and normal ExtData staging/install remain explicit
  transactions. Gallery/movie migration requires both the 3DS source `system`
  and an existing initialized Cemu `system`; only the known flags are unioned,
  while all other target bytes and other-slot shared data are retained.
  The Windows backend installs complete ExtData groups through `ReplaceFileW`,
  manifest-bound backups, and a durable recovery journal. The UI never guesses
  a Cemu MLC directory or silently installs a group. Compatibility repair
  field-updates only core/guild-card values that still match an older
  conversion; current `quest1` through `quest4` bytes are preserved. An
  incomplete optional domain never blocks core or another complete domain.

Quit Nemessix, Azahar, and Cemu before any write or rollback. See the root
[English CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) and
[Chinese CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md) for
the exact source files and transaction scope.

## Updates

The About & Updates dialog resolves the latest stable tag through the official
`MHToolkit/mh-save-sync` GitHub release page and reads its official Atom release
feed. This path does not consume the shared anonymous GitHub API quota; the
Release API remains a fallback. The first launch on each local calendar day
makes at most one silent attempt. A blocked or unavailable GitHub connection
never blocks the window or changes a local save; manual checks display the
failure and can be retried. A newer release dialog includes the release title,
publication date, notes, and official link.

The package script passes the Rust converter version into both WinUI publish
forms, so the folder, portable EXE, and installer compare the same real version
instead of the .NET default assembly version.

## Release formats

The one-command package build produces three Windows x64 formats from the same
native WinUI application and its Rust sidecar:

1. `artifacts\mh3g-save-convert-windows-x64.zip`: traditional portable folder; fully extract it before running `MH3GSaveConverter.exe`.
2. `artifacts\MH3GSaveConverter-Setup-x64.exe`: per-user installer; it installs the complete folder without requiring administrator rights.
3. `artifacts\MH3GSaveConverter-Portable-x64.exe`: one directly runnable single-file UI. It bundles the same Rust sidecar and extracts it into a per-user temporary runtime cache on first launch; it is single-file delivery, not zero extraction.

Each artifact receives its own `.sha256` sidecar. The ZIP and installer keep the complete relative layout below, including the explicit CLI launcher:

```text
MH3GSaveConverter.exe
tools/mh3g-save-convert.exe
tools/mh3g-save-convert.exe.sha256
Run-Converter.ps1
```

The job verifies the unpacked GUI executable and `tools` sidecar without
launching the GUI. `Run-Converter.ps1` is retained for explicit CLI use and
checks the bundled sidecar checksum before forwarding arguments. For development
only, the user can select a different explicit CLI path or set
`MH3G_CONVERTER_CLI`. The app treats every operation as failed when the selected
sidecar is missing or does not emit JSON.

## One-command Windows x64 package

Do not let an IDE, Qoder, or manually assembled commands build the Rust and
WinUI halves separately. From the repository root, run this **single canonical
command**:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1
```

The script preflights Windows 10 1809+/Windows 11 x64, the .NET 8 SDK,
Rust 1.95+ `cargo`/`rustup`, and Visual Studio 2022 Build Tools with
`Microsoft.VisualStudio.Component.VC.Tools.x86.x64` plus a Windows SDK. It
imports the x64 MSVC environment through `VsDevCmd.bat`, so the build never
depends on the shell which launched Qoder. Rust tests and the sidecar are
always built for `x86_64-pc-windows-msvc`, rather than accidentally inheriting
a tester's GNU default target.

Only when the machine is missing a prerequisite, explicitly allow a one-time
`winget` bootstrap (which can request administrator approval):

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1 -Bootstrap
```

`-Bootstrap` installs only missing .NET 8 SDK, Rustup, and Visual Studio 2022
C++ Build Tools (with recommended Windows SDK components). If an existing
Visual Studio/Build Tools instance is incomplete, it uses
`setup.exe modify --installPath` to add VC Tools/SDK rather than treating an
already-installed instance as a no-op `winget install`.

If WinGet says `Rustlang.Rustup` is already installed but the current user has
no usable `%USERPROFILE%\.cargo\bin\rustup.exe`, the same command repairs the
Rustup payload in place: normal WinGet install → `winget repair` → forced
Rustup installer → an official HTTPS `rustup-init.exe` fallback with a
SHA-256 sidecar integrity check. It does **not** uninstall Rustup or delete
`.cargo` / `.rustup`, does not change
the persistent PATH, and does not select a new persistent default toolchain;
the package build selects `stable-x86_64-pc-windows-msvc` only for its own
process. A 3010/1641 installer result is reported as a required restart
followed by the same command. The normal command never installs software or
changes the system. It also reuses a valid private .NET 8 SDK from an earlier
package version at `%LOCALAPPDATA%\MH3GSaveConverter\BuildTools\dotnet8\dotnet.exe`;
a prior `-NoPath` installation therefore does not trigger a second download.
Neither route clears NuGet, Cargo, or `target` caches, so repeat runs reuse
downloaded dependencies.

On success the package and its diagnostics are written to:

```text
artifacts\mh3g-save-convert-windows-x64.zip
artifacts\mh3g-save-convert-windows-x64.zip.sha256
artifacts\MH3GSaveConverter-Setup-x64.exe
artifacts\MH3GSaveConverter-Setup-x64.exe.sha256
artifacts\MH3GSaveConverter-Portable-x64.exe
artifacts\MH3GSaveConverter-Portable-x64.exe.sha256
artifacts\mh3g-save-convert.exe.sha256
artifacts\mh3g-save-convert-windows-build-transcript.txt
```

The script runs `dotnet restore`, Rust test/release builds for the fixed MSVC
target, self-contained WinUI `dotnet publish`, sidecar and ZIP SHA-256
generation, then an extracted archive/layout/sidecar check. It never launches
the GUI, Cemu, or reads a real save. Before the mandatory Rust test suite it
checks for Cemu, Cemu_release, Nemessix, and Azahar and fails early with their
names if any are running; it never terminates them. If an emulator appears
after that test, the disposable synthetic `write -> rollback` smoke is skipped
rather than touching it. Do not use `-SkipTests` or `-SkipTransactionSmoke` for
a normal distribution build.

If the UI reports `Unable to prepare patched user save`,
`tools\mh3g-save-convert.exe` came from the temporary v0.0.3 compatibility
wrapper rather than the native Rust CLI used by 0.0.4 and newer. Do not copy
`tools\compatibility-wrapper\dist\mh3g-save-convert.exe` into a WinUI package.
Delete the old `artifacts\mh3g-save-convert-windows-x64` directory and ZIP,
then rerun the exact `package-mh3g-save-converter-windows.ps1 -Bootstrap`
command above. Current packaging and the WinUI runtime both reject that legacy
wrapper before it can surface a misleading JSON error.

If it fails, send the **first failed command block** from
`artifacts\mh3g-save-convert-windows-build-transcript.txt`: begin at its `>>`
line and include the failure lines below it. Do not switch to a different manual
build command.

## Source checks on non-Windows hosts

`scripts/verify-mh3g-save-converter-windows-source.py` validates the project
metadata, XML well-formedness, argv-only bridge, JSON parsing, primary workflow
commands, CEC isolation, and bilingual copy. It is not a replacement for a
Windows SDK build.
