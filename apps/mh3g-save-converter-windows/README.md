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
- Core migration exposes `inspect` -> `convert --dry-run` -> final SHA-256
  recheck -> `convert --write`. The UI opens a confirmation dialog before the
  write and then records the emitted manifest path for manifest-bound rollback.
- The process bridge uses `ProcessStartInfo.ArgumentList`, sets
  `UseShellExecute = false`, and parses the CLI's JSON stdout. No shell command
  string is built and no conversion behavior is duplicated in C#.
- Experimental CEC is off by default. It has separate inspect, Dry Run, final
  read-only verification, write, and rollback routes. The immediately preceding
  Dry Run's aggregate source_record_set_sha256 and target_sha256_before are
  bound to its write, so a changed mailbox or cache fails closed. It never turns
  on merely because a player selected the primary slot.
- `system` and ExtData (`card*`, `cardbox`, `quest*`) remain explicit,
  independent CLI transactions in this first Windows shell. The UI does not
  guess a Cemu MLC directory or silently install an ExtData group.

Quit Nemessix, Azahar, and Cemu before any write or rollback. See the root
[English CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) and
[Chinese CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md) for
the exact source files and transaction scope.

## Release layout

The Windows release job publishes the native WinUI application and its x64 Rust
sidecar together. The extracted ZIP has this relative layout:

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
already-installed instance as a no-op `winget install`. A 3010/1641 installer
result is reported as a required restart followed by the same command. The
normal command never installs software or changes the system. Neither route
clears NuGet, Cargo, or `target` caches, so repeat runs reuse downloaded
dependencies.

On success the package and its diagnostics are written to:

```text
artifacts\mh3g-save-convert-windows-x64.zip
artifacts\mh3g-save-convert-windows-x64.zip.sha256
artifacts\mh3g-save-convert.exe.sha256
artifacts\mh3g-save-convert-windows-build-transcript.txt
```

The script runs `dotnet restore`, Rust test/release builds for the fixed MSVC
target, self-contained WinUI `dotnet publish`, sidecar and ZIP SHA-256
generation, then an extracted archive/layout/sidecar check. It never launches
the GUI, Cemu, or reads a real save. When no emulator is running it additionally
runs a disposable synthetic `write -> rollback` smoke; when one is running it
leaves it untouched and skips that synthetic write. Do not use `-SkipTests` or
`-SkipTransactionSmoke` for a normal distribution build.

If it fails, send the **first** `error`, `MSB`, `link.exe`, or `cargo` line from
`artifacts\mh3g-save-convert-windows-build-transcript.txt` instead of
switching to a different manual build command.

## Source checks on non-Windows hosts

`scripts/verify-mh3g-save-converter-windows-source.py` validates the project
metadata, XML well-formedness, argv-only bridge, JSON parsing, primary workflow
commands, CEC isolation, and bilingual copy. It is not a replacement for a
Windows SDK build.
