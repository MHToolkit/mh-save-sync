# MH3G Converter UI Options

- Access date: 2026-07-26
- Scope: a local Japanese MH3G 3DS to Wii U/Cemu converter UI for Windows,
  macOS, and optionally Android.
- Non-goal: changing the validated conversion rules or adding automatic save
  synchronization.

## Current repository constraints

The validated converter is the Rust crate `mh3g-save-convert`. Its CLI already
owns profile validation, dry-run reports, and SHA-256 reporting. Its
transaction support is component-specific:

- `convert` and `convert-system` have atomic target replacement, backups,
  manifests, and rollback.
- Experimental `convert-cec` has its own `cec` backup, manifest, and rollback.
- `convert-extras` only writes all eight converted `card*`/`quest*` files into
  a new staging directory. It refuses an existing component output and has no
  installer, target backup, manifest, batch commit, or rollback for those
  files.

The stopped-emulator process guard is currently implemented only on macOS. The
non-macOS `MacOsProcessProbe` path returns no matching process, so the Windows
CLI does not itself prove that Cemu, Azahar, or Nemessix is stopped. A UI must
reuse the validated parsing and transformation operations, but it must not
present these missing Windows and multi-file transaction capabilities as if
the current CLI already supplied them.

The wider repository has already accepted native SwiftUI/AppKit and
Kotlin/Compose shells over shared Rust logic in ADR 0001. That remains the
cloud synchronization client's `save-client` and UniFFI Kotlin/Swift bridge
decision. The converter crate is currently a Rust library plus CLI, but it does
not yet expose a stable UniFFI or C ABI designed for a converter UI.

## Options

### Option A: Tauri 2 for Windows and macOS, Compose for Android (historical comparison)

Use one small Tauri 2 desktop application that invokes typed Rust converter
library functions directly. Keep Android as a native Compose application using
Android's Storage Access Framework (SAF) and a narrow Rust bridge.

Advantages:

- Reuses the Rust converter directly on both desktop platforms.
- Shares the complete desktop workflow and most presentation code.
- Produces normal Windows installers and a macOS application bundle.
- Avoids maintaining a second conversion engine or parsing CLI text.

Costs:

- Adds a web frontend toolchain and WebView runtime behavior to this repository.
- Windows and macOS still require separate signing identities and release jobs.
- Android cannot safely reuse desktop path selection; SAF permissions and
  document-tree access remain native concerns.

This was the earlier recommended route for a converter-specific desktop app.
It is retained here as historical comparison only. ADR 0014 rejects the
WebView/Tauri exception: do not implement this route.

### Option B: native SwiftUI and WinUI 3 shells (accepted)

Keep the desktop UIs native: a standalone SwiftUI `WindowGroup` application on
macOS and a standalone C#/.NET 8 WinUI 3 application on Windows. Each packages
`mh3g-save-convert`, launches it through a strict argv array, and consumes its
machine-readable JSON. Android is deferred.

The converter UI defaults to the system language. Its settings provide a
persisted locale override between Simplified Chinese and English.

Advantages:

- Best platform-native file pickers, accessibility, window behavior, and
  signing integration.
- Respects ADR 0001's native-shell/WebView boundary while leaving its cloud-sync
  `save-client` UniFFI Kotlin/Swift bridge unchanged.
- Avoids Vue, WebView, Tauri, and Electron in the converter UI.
- Keeps converter, transaction, backup, and rollback logic in one bundled CLI.

Costs:

- Two desktop presentation implementations and two native packaging paths.
- WinUI packaging adds the largest new maintenance surface.
- Slower route to a Windows UI, which is the platform with the immediate CLI
  usability problem.

This is the accepted route. It must not begin desktop Write support until the
backend safety prerequisites below are implemented.

### Option C: Flutter for all three platforms

Build one Flutter UI and call the Rust converter through `dart:ffi` or a bridge
generator.

Advantages:

- One presentation stack across Windows, macOS, and Android.
- Official Flutter deployment support covers all three target platforms.

Costs:

- Introduces a third client stack alongside existing SwiftUI and Compose code.
- Requires a new Rust/Dart bridge, packaging pipeline, and contributor toolchain.
- Reuses less of the current repository than either option A or B.

This is not recommended for the current repository.

## Recommended delivery sequence

1. Stabilize a JSON CLI contract for inspection, conversion planning, write,
   and rollback. Its reports must contain exact source and target paths,
   profiles, hashes, files to be modified, backup paths, manifest paths, and
   structured error codes. The desktop shells launch only the bundled
   `mh3g-save-convert` through strict argv arrays and consume this JSON; they do
   not use a converter Rust UI bridge or reimplement backend operations. This
   converter-only contract does not supersede ADR 0001's cloud-sync UniFFI
   Kotlin/Swift bridge.
2. Implement backend prerequisites before exposing Write: native Windows
   fail-closed emulator-process detection, a backend-issued dry-run hash
   authorization, and transactional ExtData batches. `card1`/`card2`/`card3`/
   `cardbox` are one indivisible guild-card install and rollback group;
   `quest1` through `quest4` are one indivisible quest install and rollback
   group. CEC is disabled by default and remains experimental.
3. Build independent macOS SwiftUI `WindowGroup` and Windows C#/.NET 8 WinUI 3
   converter applications. Do not use Vue, WebView, Tauri, or Electron.
4. Add Windows signing/MSIX or signed installer work and macOS Developer ID,
   hardened runtime, and notarization. An unsigned development ZIP remains a
   testing artifact, not the final distribution format.
5. Add Android only after the desktop workflow is stable and device connectivity
   is proven. Android should export a converted directory or ZIP chosen through
   SAF; it should not pretend it can directly install into a desktop Cemu MLC
   path. MCP is not part of version 0.1.

## Required user workflow

The first screen is the actual conversion workbench, not a landing page.

1. Select the source slot file and matching target slot file.
2. Show the detected Japanese profiles, slot number, sizes, and SHA-256 values.
3. Present optional component groups independently: shared `system`, the
   indivisible guild-card group, the indivisible downloaded-quest group, and
   disabled-by-default experimental CEC.
4. Run a mandatory dry-run and show the exact file write set. A component not
   selected must not appear in that set. Write remains disabled until the
   backend issues a dry-run hash authorization bound to the reviewed source,
   target, staged output, and selection.
5. Require a platform-backed emulator-stopped gate before enabling Write. On
   macOS the facade may reuse the existing process probe. On Windows it must
   detect the supported Cemu/Cemu_release, Azahar, and Nemessix processes and
   fail closed when process state cannot be established; an instructional
   checkbox is not a process guard.
6. For the guild-card group (`card1`, `card2`, `card3`, and `cardbox`) or the
   quest group (`quest1` through `quest4`), first convert the complete group
   into a fresh staging directory, then use a batch transaction to snapshot
   every destination, verify staged hashes, install the complete group, and
   restore the complete pre-write group if any file fails. The UI must not
   select, install, or roll back only one member of either group. Do not copy
   these files directly from the staging directory and call that a successful
   transaction.
7. After Write, show every backup and manifest path with a Roll Back action.
   The batch manifest must bind every selected destination to its before/after
   hash and backup or previously-absent state, so rollback cannot restore only
   part of a guild-card/quest group.

The UI must not expose a single action that silently converts or overwrites an
entire save directory. It may accept a directory as input for convenience, but
it must resolve it into the documented component list and show that list before
any write.

## Error and release requirements

- File errors must identify the operation, full path, OS error code, and whether
  the failure happened while reading the source, creating a backup, replacing a
  target, or writing a manifest.
- Windows packages must be extracted before use. The release should document
  Mark-of-the-Web unblocking for unsigned test builds and should use code signing
  for public builds.
- Desktop builds need an automated `--help` or facade smoke test on the target
  OS plus archive extraction and launch verification.
- UI tests must prove that deselected component groups do not reach the write
  plan, neither ExtData group can be partially selected, and Write remains
  disabled until dry-run hash authorization succeeds.
- Localization acceptance must include complete Simplified Chinese and English
  resources, tests for the system-language default, and a persisted settings
  locale override.
- Windows tests must start a supported emulator-named process and prove that
  writes and rollback fail closed before touching any target.
- Batch-install tests must inject a failure after at least one selected
  `card*`/`quest*` target was staged or replaced, then prove that every target,
  backup, and manifest returns to the documented pre-write state.

## Sources

- Repository ADR 0001: `docs/adr/0001-client-stack.md`
- Repository ADR 0013: `docs/adr/0013-mh3g-cross-format-conversion.md`
- Tauri 2 guide: <https://v2.tauri.app/start/>
- Flutter supported platforms:
  <https://docs.flutter.dev/reference/supported-platforms>
- WinUI 3 overview:
  <https://learn.microsoft.com/windows/apps/winui/winui3/>
