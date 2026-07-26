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

## Build on Windows

Install Visual Studio 2022 with the **.NET desktop development** workload,
Windows 10/11 SDK, and .NET 8 SDK. From the repository root:

```powershell
dotnet restore apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj
cargo build --locked --release -p mh3g-save-convert --bin mh3g-save-convert
$publish = "artifacts\mh3g-save-convert-windows-x64"
dotnet publish apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj -c Release -r win-x64 --self-contained true -p:Platform=x64 -p:WindowsAppSDKSelfContained=true -o $publish
New-Item -ItemType Directory -Force "$publish\tools"
Copy-Item target\release\mh3g-save-convert.exe "$publish\tools\mh3g-save-convert.exe"
$sidecarHash = (Get-FileHash -Algorithm SHA256 "$publish\tools\mh3g-save-convert.exe").Hash.ToLowerInvariant()
"$sidecarHash  mh3g-save-convert.exe" | Set-Content -NoNewline -Encoding ascii "$publish\tools\mh3g-save-convert.exe.sha256"
Copy-Item scripts\mh3g-windows-launcher.ps1 "$publish\Run-Converter.ps1"
```

Generate and verify the sidecar SHA-256 before distributing a locally assembled
package. The GitHub Windows workflow performs this assembly and retains the
Rust binary's provenance alongside the UI package.

## Source checks on non-Windows hosts

`scripts/verify-mh3g-save-converter-windows-source.py` validates the project
metadata, XML well-formedness, argv-only bridge, JSON parsing, primary workflow
commands, CEC isolation, and bilingual copy. It is not a replacement for a
Windows SDK build.
