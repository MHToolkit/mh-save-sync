# ADR 0014: Native MH3G converter UI shells

- Status: Accepted for phase1-alpha
- Date: 2026-07-26
- Owners: MHToolkit maintainers
- Review date: 2026-10-26

## Decision question

How should the local MH3G converter be presented on desktop without widening
the validated converter's safety boundary or creating a second conversion
implementation?

## Context

`mh3g-save-convert` owns profile validation, conversion, dry-run reporting,
installation, backups, manifests, and rollback. A GUI is useful for selecting
the documented source and target components, but it must not reinterpret those
operations or make an unsafe write appear safe.

The current backend does not yet meet every desktop-write prerequisite. In
particular, Windows must establish that supported emulator processes are
stopped before any write or rollback. ExtData installation also needs
transactional grouping rather than independent copies of staged files.

## Decision

Ship two independent desktop applications:

- macOS is a standalone SwiftUI application with a `WindowGroup` scene.
- Windows is a standalone C#/.NET 8 application using WinUI 3.

Neither application uses Vue, a WebView, Tauri, or Electron. They are native
shells for the converter workbench, not a shared web frontend.

Each application packages `mh3g-save-convert` and starts it only with a strict
argv array. The macOS shell uses `Process.arguments`; the Windows shell uses
`ProcessStartInfo.ArgumentList`. Neither shell constructs a command string or
invokes a shell. The shell consumes the converter's machine-readable JSON
results and maps UI selections only to documented CLI arguments. A stable JSON
mode is therefore an implementation prerequisite of the UI contract.

The desktop applications must not reimplement conversion, profile parsing,
transaction handling, backup creation, manifest construction, or rollback.
Those behaviors remain in the bundled converter process; a UI may display its
JSON plan and error results but may not substitute its own file operations.

### Bridge scope

This CLI-plus-JSON process boundary applies only to the bundled
`mh3g-save-convert` foreground converter UI. It does not modify or supersede
ADR 0001's cloud synchronization client decision: that client retains its
shared Rust `save-client` core and its UniFFI Kotlin/Swift bridge (or the
separately approved narrow C ABI fallback).

Android is deferred until the desktop flow is stable and device connectivity is
proven. Android's Storage Access Framework and delivery model need a separate
decision. MCP is not part of version 0.1 and is not implemented by this ADR.

### Language

Both native applications default to the system language. Their settings must
provide a persisted locale override for Simplified Chinese and English, so a
user can switch between the two without changing the operating-system setting.

### Backend safety prerequisites

Desktop Write and Roll Back remain unavailable until the converter provides all
of the following backend-enforced guarantees:

- On Windows, the supported emulator process probe is fail-closed. A write or
  rollback is refused when the state of Cemu, Azahar, or Nemessix cannot be
  established; a UI checkbox is not an equivalent guard.
- `card1`, `card2`, `card3`, and `cardbox` form one indivisible guild-card
  ExtData installation and rollback group. `quest1` through `quest4` form one
  indivisible quest ExtData installation and rollback group. Each selected
  group is backed up, installed, manifested, and restored as a complete unit.
  A failure restores that complete group rather than leaving a partial install.
- CEC is disabled by default and remains explicitly experimental. It requires a
  separate opt-in and does not join the default Write path.
- A Write requires a backend-issued dry-run hash authorization. The subsequent
  write must be bound to the reviewed source, target, staged-output, and
  selection hashes, and must be rejected when any of them change.

## Alternatives

### Tauri 2 desktop shell

Tauri 2 was considered because it could share a web presentation across macOS
and Windows. It is rejected for this converter: the native desktop split keeps
file-picker, accessibility, process, signing, and packaging behavior in the
platform shells and avoids a Vue/WebView runtime.

### Electron desktop shell

Electron is rejected for the same WebView boundary and runtime-maintenance
cost. It does not reduce the need for per-platform write safety or signing.

### Shared Rust UI bridge

Direct UniFFI or C ABI calls for this converter UI would make it a second
integration surface for transactional behavior. The CLI plus JSON contract is
the single process boundary for this converter in phase1-alpha; this does not
reject ADR 0001's cloud synchronization client bridge.

## Security and data-integrity impact

The argv-only process boundary prevents shell interpolation and makes each
requested operation auditable. JSON is display and orchestration data, not
authority to bypass converter checks. The converter retains exclusive authority
for process guards, hash authorization, backup, atomic installation, manifest
validation, and rollback.

No UI may call a write before a successful dry-run authorization, or present a
partial ExtData group as an installed or recoverable result. CEC stays outside
the ordinary path until its experimental evidence is sufficient for a separate
acceptance decision.

## Migration and rollback

This ADR changes no existing save data and does not introduce a GUI write path
before the backend prerequisites exist. Early desktop builds are inspect and
dry-run only. Once the prerequisites are implemented, every write is reverted
through the converter's manifest-bound rollback operation; the UI only invokes
and displays that result.

## Verification

- `rtk proxy python3 scripts/ux-research-link-check.py --doc docs/research/MH3G_CONVERTER_UI_OPTIONS.md --output artifacts/research/mh3g_converter_ui_link_check.json`
- `rtk grep -n 'WinUI 3' docs/adr/0014-mh3g-native-converter-ui.md docs/research/MH3G_CONVERTER_UI_OPTIONS.md`
- `rtk git diff --check`

Implementation acceptance additionally requires platform tests proving strict
argv invocation and JSON consumption, Windows fail-closed process probing,
dry-run hash authorization, and complete group rollback after an injected
ExtData failure. It also requires complete Simplified Chinese and English
resources, plus tests proving the system-language default and the settings
locale override.
