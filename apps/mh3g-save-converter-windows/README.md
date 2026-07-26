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
  read-only verification, write, and rollback routes. It never turns on merely
  because a player selected the primary slot.
- `system` and ExtData (`card*`, `cardbox`, `quest*`) remain explicit,
  independent CLI transactions in this first Windows shell. The UI does not
  guess a Cemu MLC directory or silently install an ExtData group.

Quit Nemessix, Azahar, and Cemu before any write or rollback. See the root
[English CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md) and
[Chinese CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md) for
the exact source files and transaction scope.

## Sidecar layout

The release packager must put the signed x64 Rust binary at this relative path:

```text
MH3GSaveConverter.exe
tools/mh3g-save-convert.exe
```

For development only, the user can select a different explicit CLI path or set
`MH3G_CONVERTER_CLI`. The app treats every operation as failed when the selected
sidecar is missing or does not emit JSON.

## Build on Windows

Install Visual Studio 2022 with the **.NET desktop development** workload,
Windows 10/11 SDK, and .NET 8 SDK. From the repository root:

```powershell
dotnet restore apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj
dotnet build apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj -c Release -p:Platform=x64
dotnet publish apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj -c Release -r win-x64 --self-contained true -p:Platform=x64 -p:WindowsAppSDKSelfContained=true
```

Copy the release-built `mh3g-save-convert.exe` into the output's `tools`
directory before running the app. Packaging/signing is intentionally separate
from this UI project so that the Rust binary's provenance stays visible.

## Source checks on non-Windows hosts

`scripts/verify-mh3g-save-converter-windows-source.py` validates the project
metadata, XML well-formedness, argv-only bridge, JSON parsing, primary workflow
commands, CEC isolation, and bilingual copy. It is not a replacement for a
Windows SDK build.
