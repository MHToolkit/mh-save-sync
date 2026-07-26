# MH3G Save Converter Native UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a safe, bilingual, native macOS and Windows workbench for the existing Japanese MH3G 3DS-to-Cemu converter without duplicating or weakening the Rust conversion and transaction rules.

**Architecture:** Keep `mh3g-save-convert` as the sole parser, converter, transaction owner, and JSON report authority. First add a cross-platform fail-closed emulator guard and an ExtData group installer to the Rust crate. Then build independent SwiftUI and WinUI 3 shells which call the bundled CLI with an argv array, drain stdout/stderr asynchronously, parse its JSON, and gate UI write actions on a current dry-run fingerprint.

**Tech Stack:** Rust 2024 / Clap / Serde / `windows-sys`; Swift 6 / SwiftUI / AppKit; C# / .NET 8 / WinUI 3 / Windows App SDK; deterministic Python and Swift asset generators; GitHub Actions Windows/macOS packaging.

---

## File and responsibility map

| Path | Responsibility |
| --- | --- |
| `docs/adr/0014-mh3g-native-converter-ui.md` | Records the native-shell, CLI-bridge, safety-gate, rollback and Android deferral decision. |
| `docs/research/MH3G_CONVERTER_UI_OPTIONS.md` | Corrects the obsolete Tauri recommendation to the approved native SwiftUI + WinUI 3 decision. |
| `crates/mh3g-save-convert/src/process_probe.rs` | Owns platform process enumeration and the fail-closed `ProcessProbe` abstraction. |
| `crates/mh3g-save-convert/src/extras_transaction.rs` | Owns validated ExtData group staging installation, backup, manifest, rollback and all-or-nothing recovery. |
| `crates/mh3g-save-convert/src/{lib.rs,main.rs,transaction.rs,cec.rs}` | Re-exports the safety APIs, uses the platform guard for every write/rollback, binds a UI write to current source/target hashes, and exposes stable JSON CLI operations. |
| `crates/mh3g-save-convert/tests/{process_probe.rs,extras_transaction.rs,cli.rs}` | Synthetic, no-save-content regression coverage for platform gates, group atomicity and CLI JSON contract. |
| `scripts/generate-mh3g-save-converter-assets.py` | Generates the self-authored `3DS -> HD` icon and five low-contrast scene illustrations deterministically. |
| `apps/mh3g-save-converter-macos/` | Standalone SwiftPM `WindowGroup` App, view model, async process client, bilingual resources, unit tests and generated artwork. |
| `scripts/{build,package,mh3g-save-converter-macos-smoke}.sh` | Builds, signs ad hoc, packages and safely smoke-tests the macOS app with only synthetic fixtures. |
| `apps/mh3g-save-converter-windows/` | Standalone .NET 8 / WinUI 3 solution, pure testable workflow core, Mica UI, localization and icon. |
| `scripts/package-mh3g-save-converter-windows.ps1` | Publishes a self-contained x64 Windows app plus the matching static CLI and checksum manifest. |
| `.github/workflows/mh3g-converter-ui-{macos,windows}.yml` | Target-native build, test, packaged-sidecar and archive verification. |
| `docs/MH3G_SAVE_CONVERTER_{UI,AI_CLI}_GUIDE.{md,zh-CN.md}` | Bilingual installation, input shape, dry-run, rollback, CEC and AI/CLI guidance. |
| `scripts/mh3g-test-artifact-inventory.py` | Read-only, SHA-256 inventory for later safe test-output cleanup; it never removes user data. |

## Task 1: Align the accepted decision records before implementation

**Files:**
- Create: `docs/adr/0014-mh3g-native-converter-ui.md`
- Modify: `docs/research/MH3G_CONVERTER_UI_OPTIONS.md`
- Modify: `docs/DECISIONS.md`
- Test: `scripts/ux-research-link-check.py`

- [ ] **Step 1: Write the documentation acceptance check before changing the decision.**

Create a short Python-free shell assertion in the task transcript and run it against the current document:

```bash
rtk grep -n 'recommended route for a converter-specific desktop app' docs/research/MH3G_CONVERTER_UI_OPTIONS.md
```

Expected: one match recommending Tauri, proving the document contradicts the approved decision.

- [ ] **Step 2: Replace the obsolete recommendation with the exact approved architecture.**

Add ADR 0014 with these non-negotiable statements:

```markdown
## Decision

Use independent native shells: SwiftUI `WindowGroup` on macOS and .NET 8 / WinUI 3 on Windows. Both bundle and invoke the exact same-platform `mh3g-save-convert` executable through an argv array and structured JSON reports. Neither shell owns byte transforms, transaction rules, emulator checks, backups, manifests, or rollback.

The `card1`/`card2`/`card3`/`cardbox` group and the `quest1`/`quest2`/`quest3`/`quest4` group are indivisible install and rollback units. CEC remains separately labelled experimental and is disabled by default.
```

Change the final recommendation in the research document from Tauri to Option B, and explain that its former delivery-speed advantage no longer outweighs the explicit native-control requirement. Add ADR 0014 to `docs/DECISIONS.md`.

- [ ] **Step 3: Run the documentation checks.**

Run:

```bash
rtk proxy python3 scripts/ux-research-link-check.py
rtk grep -n 'WinUI 3' docs/adr/0014-mh3g-native-converter-ui.md docs/research/MH3G_CONVERTER_UI_OPTIONS.md
rtk grep -n 'Tauri 2.*recommended' docs/research/MH3G_CONVERTER_UI_OPTIONS.md
```

Expected: link checker succeeds, both native-decision searches match, and the last command has no output.

- [ ] **Step 4: Commit the decision record.**

```bash
rtk git add docs/adr/0014-mh3g-native-converter-ui.md docs/research/MH3G_CONVERTER_UI_OPTIONS.md docs/DECISIONS.md
rtk git commit -m 'docs(mh3g): record native converter UI decision'
```

### Task 2: Add a cross-platform fail-closed emulator process gate

**Files:**
- Create: `crates/mh3g-save-convert/src/process_probe.rs`
- Modify: `crates/mh3g-save-convert/Cargo.toml`
- Modify: `crates/mh3g-save-convert/src/lib.rs`
- Modify: `crates/mh3g-save-convert/src/transaction.rs`
- Modify: `crates/mh3g-save-convert/src/cec.rs`
- Create: `crates/mh3g-save-convert/tests/process_probe.rs`

- [ ] **Step 1: Write the failing platform-independent probe tests.**

Define a test-only `StaticEnumerator` that returns a supplied list or an error. Write these tests against the desired public API:

```rust
use mh3g_save_convert::process_probe::{ProcessEnumerator, ProcessProbe, PlatformProcessProbe};

#[test]
fn rejects_supported_windows_emulator_name_case_insensitively() {
    let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::names(["Cemu_release.EXE"]));
    assert_eq!(probe.matching_process().unwrap().as_deref(), Some("Cemu_release.EXE"));
}

#[test]
fn rejects_supported_native_frontends() {
    for name in ["Cemu.exe", "Nemessix.exe", "Azahar.exe"] {
        let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::names([name]));
        assert_eq!(probe.matching_process().unwrap().as_deref(), Some(name));
    }
}

#[test]
fn enumeration_error_is_not_treated_as_no_running_emulator() {
    let probe = PlatformProcessProbe::with_enumerator(StaticEnumerator::failure("snapshot failed"));
    assert!(probe.matching_process().is_err());
}
```

- [ ] **Step 2: Run the new tests and confirm the intended RED failure.**

Run:

```bash
rtk cargo test --locked -p mh3g-save-convert --test process_probe
```

Expected: compile failure because `process_probe` and `PlatformProcessProbe` do not exist yet.

- [ ] **Step 3: Implement the smallest explicit process-probe module.**

Create `process_probe.rs` with a public `ProcessProbe` trait and a `PlatformProcessProbe` whose only observable method is:

```rust
pub trait ProcessProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError>;
}

pub const GUARDED_PROCESS_NAMES: [&str; 8] = [
    "Nemessix", "nemessix", "Azahar", "azahar",
    "Cemu", "cemu", "Cemu_release", "cemu_release",
];
```

Implement macOS enumeration with `pgrep -x` exactly as the existing code does. Implement Windows enumeration with `CreateToolhelp32Snapshot`, `Process32FirstW`, and `Process32NextW` from a target-specific `windows-sys` dependency. Convert `szExeFile` to a Rust string, compare names with `eq_ignore_ascii_case`, and recognise both `Cemu.exe`/`Cemu_release.exe`/`Nemessix.exe`/`Azahar.exe` and their no-extension macOS forms. On any snapshot or enumeration error return `ConversionError::IoAtPath`; on unsupported host OS return `ConversionError::UnsafeInstall("cannot establish emulator process state on this platform")`, never `Ok(None)`.

Move the old trait and macOS implementation out of `transaction.rs`; make `install`, `rollback`, `install_cec`, and `rollback_cec` instantiate `PlatformProcessProbe`. Preserve `install_with` and add `install_cec_with` / `rollback_cec_with` for deterministic tests.

- [ ] **Step 4: Verify the green suite and non-regression.**

Run:

```bash
rtk cargo test --locked -p mh3g-save-convert --test process_probe
rtk cargo test --locked -p mh3g-save-convert
```

Expected: all probe tests pass and the existing converter suite stays green.

- [ ] **Step 5: Commit the process gate.**

```bash
rtk git add crates/mh3g-save-convert/Cargo.toml crates/mh3g-save-convert/src/process_probe.rs crates/mh3g-save-convert/src/lib.rs crates/mh3g-save-convert/src/transaction.rs crates/mh3g-save-convert/src/cec.rs crates/mh3g-save-convert/tests/process_probe.rs Cargo.lock
rtk git commit -m 'feat(mh3g): fail closed on emulator process probes'
```

### Task 3: Implement transactional ExtData group install and rollback

**Files:**
- Create: `crates/mh3g-save-convert/src/extras_transaction.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`
- Modify: `crates/mh3g-save-convert/src/main.rs`
- Create: `crates/mh3g-save-convert/tests/extras_transaction.rs`
- Modify: `crates/mh3g-save-convert/tests/cli.rs`

- [ ] **Step 1: Write tests for the public all-or-nothing contract.**

Use only generated header-bearing component fixtures. The first test file must cover these exact behaviours:

```rust
#[test]
fn guild_card_selection_requires_all_four_components() { /* card1/card2/card3/cardbox */ }

#[test]
fn quest_selection_requires_all_four_components() { /* quest1..quest4 */ }

#[test]
fn dry_run_does_not_create_targets_backups_or_manifest() { /* selected full group */ }

#[test]
fn failed_second_replacement_retains_a_recovery_journal_for_explicit_rollback() { /* injected writer failure */ }

#[test]
fn rollback_restores_every_initialized_target() { /* full recovery journal */ }

#[test]
fn running_emulator_rejects_extra_install_before_any_target_changes() { /* static active probe */ }

#[test]
fn changed_source_or_target_set_hash_rejects_write_before_any_target_changes() { /* dry-run token */ }
```

Use a test `ExtraFileOperations` implementation that fails when attempting the second selected exchange. Assert that the first completed target remains recoverable through the retained journal, then run explicit rollback and assert byte-for-byte equality with every pre-install target and zero transaction artifacts. Add a race test that changes a valid target at the exchange seam; the install must reject and preserve that later value.

- [ ] **Step 2: Confirm the batch tests fail because the API is absent.**

Run:

```bash
rtk cargo test --locked -p mh3g-save-convert --test extras_transaction
```

Expected: compile failure for `ExtraGroup`, `install_extra_groups_with`, and `rollback_extra_groups_with`.

- [ ] **Step 3: Implement the manifest-bound group transaction.**

Create these public types and APIs in `extras_transaction.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
pub enum ExtraGroup { GuildCards, Quests }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtraInstallEntry {
    pub group: ExtraGroup,
    pub target: PathBuf,
    pub temporary: PathBuf,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub backup: Option<PathBuf>,
    pub target_previously_existed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtraInstallManifest {
    pub version: u32,
    pub transaction_id: String,
    pub staging_dir: PathBuf,
    pub target_dir: PathBuf,
    pub groups: Vec<ExtraGroup>,
    pub entries: Vec<ExtraInstallEntry>,
}
```

`install_extra_groups_with(staging_dir, target_dir, groups, expected_staging_set_sha256, expected_target_set_sha256, probe, operations)` must normalize paths, reject duplicates and symlinks, require each requested complete group, and require every selected target component to already exist as a valid Cemu file from an initialized Wii U/Cemu save. It verifies every staged component's Cemu wrapper and SHA-256 before changing targets, calculates and requires the exact expected staging-set and before-target-set SHA-256 values, acquires one directory-bound lock, then creates a canonical UUID transaction ID and pre-plans every controlled temporary path. It must create `.mh3g-extra-recovery.json` with create-new semantics before creating any backup or temporary file, sync the journal, snapshot every selected target, stage every selected output at its manifest-bound temporary path, and sync the complete recovery material before the first target exchange. Replacement and rollback use a platform atomic swap (`renamex_np(RENAME_SWAP)` on macOS or `renameat2(RENAME_EXCHANGE)` on Linux/Android), verify both displaced values, and fail closed while retaining the journal on any uncertain exchange. A successful installation retains the create-new recovery journal as its sole active rollback record; it must not promote then unlink it. Windows UI must disable optional multi-file ExtData writes until it has an equivalent durable directory metadata barrier.

`rollback_extra_groups_with(manifest, probe, operations)` must verify manifest version, normalized controlled paths, group completeness, per-entry hashes and controlled backup paths before restoring the complete entry set. It must consume all backups and the recovery journal only after every target restoration succeeds; a conflicting target remains untouched and leaves the journal for a later retry.

Keep `convert-extras` as a staging-only conversion. Extend every UI-facing write command with optional optimistic-concurrency flags: `convert` and `convert-system` accept `--expected-source-sha256` plus `--expected-target-sha256`; `convert-extras` accepts `--expected-source-set-sha256`; `install-extras` accepts `--expected-staging-set-sha256` plus `--expected-target-set-sha256`; `convert-cec` accepts `--expected-source-record-set-sha256` plus `--expected-target-sha256`. A supplied value is compared after all required inputs are read and before any backup, temporary file or target change. The existing CLI remains compatible when those optional flags are omitted, while both UI shells always provide them after a successful dry-run. Add `install-extras --staging-dir <dir> --target-dir <dir> --groups guild-cards,quests [--expected-staging-set-sha256 <sha256>] [--expected-target-set-sha256 <sha256>] [--dry-run|--write]` and `rollback-extras --manifest <path>`; no command accepts an individual `card#` or `quest#` path.

- [ ] **Step 4: Add JSON reports and CLI integration tests.**

Introduce `ExtraInstallReport` with `operation`, `status`, `groups`, `staging_dir`, `target_dir`, `staging_set_sha256`, `target_set_sha256_before`, `entries`, `backup_paths` and `manifest`. Extend existing reports only by additive fields (`operation`, `source_sha256`, `target_sha256_before`, `write_set`, `not_written`) so existing JSON consumers can continue reading their old keys. Add CLI tests that parse:

```rust
let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
assert_eq!(value["operation"], "install-extras");
assert_eq!(value["status"], "dry-run");
assert_eq!(value["entries"].as_array().unwrap().len(), 4);
```

- [ ] **Step 5: Run the transaction, CLI and full crate suite.**

Run:

```bash
rtk cargo test --locked -p mh3g-save-convert --test extras_transaction
rtk cargo test --locked -p mh3g-save-convert --test cli
rtk cargo test --locked -p mh3g-save-convert
```

Expected: all tests pass; no test accesses a real save, MLC or emulator.

- [ ] **Step 6: Commit the ExtData transaction.**

```bash
rtk git add crates/mh3g-save-convert/src/extras_transaction.rs crates/mh3g-save-convert/src/lib.rs crates/mh3g-save-convert/src/main.rs crates/mh3g-save-convert/tests/extras_transaction.rs crates/mh3g-save-convert/tests/cli.rs
rtk git commit -m 'feat(mh3g): install extdata groups transactionally'
```

### Task 4: Generate self-authored, redistributable imagery and the converter icon

**Files:**
- Create: `scripts/generate-mh3g-save-converter-assets.py`
- Create: `scripts/verify-mh3g-save-converter-assets.py`
- Create: `apps/mh3g-save-converter-macos/Resources/Artwork/`
- Create: `apps/mh3g-save-converter-windows/assets/`
- Modify: `.gitignore`

- [ ] **Step 1: Write a failing deterministic-assets test.**

Create `scripts/verify-mh3g-save-converter-assets.py` so it expects these files after generation:

```text
apps/mh3g-save-converter-macos/Resources/Artwork/input-route.png
apps/mh3g-save-converter-macos/Resources/Artwork/components-workshop.png
apps/mh3g-save-converter-macos/Resources/Artwork/dry-run-flow.png
apps/mh3g-save-converter-macos/Resources/Artwork/rollback-harbor.png
apps/mh3g-save-converter-macos/Resources/Artwork/cec-mailbox.png
apps/mh3g-save-converter-macos/Resources/AppIcon/MH3GSaveConverter.icns
apps/mh3g-save-converter-windows/assets/MH3GSaveConverter.ico
```

The verifier must check PNG dimensions, non-zero bytes, and that the icon contains sizes 16, 32, 48, 256. Run it before implementation:

```bash
rtk proxy python3 scripts/verify-mh3g-save-converter-assets.py
```

Expected: failure listing the missing generated files.

- [ ] **Step 2: Implement only self-authored artwork.**

Create a standard-library-only Python generator that draws an abstract deep-teal island horizon, water, warm amber route line, save-file shapes and semantic landmarks for each phase. Do not download screenshots, logos, character art or source images. Use the same deep-teal and amber geometry to render a 1024px `3DS -> HD` double-save icon, generate all macOS iconset PNGs, call `iconutil -c icns` when available, and write a valid multi-image `.ico` directly for Windows.

The script must accept `--repo-root <absolute path>` and fail if an output would be outside the repository. Store only final generated assets in the repository; add transient `.iconset` directories to `.gitignore`.

- [ ] **Step 3: Verify reproducibility and inspect the assets.**

Run:

```bash
rtk proxy python3 scripts/generate-mh3g-save-converter-assets.py --repo-root "$PWD"
rtk proxy python3 scripts/verify-mh3g-save-converter-assets.py
rtk proxy shasum -a 256 apps/mh3g-save-converter-macos/Resources/Artwork/*.png apps/mh3g-save-converter-macos/Resources/AppIcon/MH3GSaveConverter.icns apps/mh3g-save-converter-windows/assets/MH3GSaveConverter.ico
```

Expected: verifier succeeds and hashes are stable on an immediate second generator run.

- [ ] **Step 4: Commit visual assets and generator.**

```bash
rtk git add .gitignore scripts/generate-mh3g-save-converter-assets.py scripts/verify-mh3g-save-converter-assets.py apps/mh3g-save-converter-macos/Resources apps/mh3g-save-converter-windows/assets
rtk git commit -m 'feat(mh3g): add converter visual assets'
```

### Task 5: Build the macOS workflow core with tests first

**Files:**
- Create: `apps/mh3g-save-converter-macos/Package.swift`
- Create: `apps/mh3g-save-converter-macos/Sources/ConverterPresentation/{ConverterCommand.swift,ConverterCommandClient.swift,ConversionWorkflow.swift,ConversionTypes.swift}`
- Create: `apps/mh3g-save-converter-macos/Tests/ConverterPresentationTests/{ConversionWorkflowTests.swift,ConverterCommandClientTests.swift}`

- [ ] **Step 1: Write failing Swift state-machine tests.**

Use a `FakeConverterCommandExecutor` that records argv and returns synthetic JSON. Add these tests before production code:

```swift
@MainActor func testWriteIsDisabledBeforeDryRun() async
@MainActor func testDryRunAuthorizesOnlyTheExactInputFingerprint() async
@MainActor func testChangingTargetInvalidatesDryRunAuthorization() async
@MainActor func testDeselectedExtraGroupNeverAppearsInWritePlan() async
@MainActor func testExperimentalCECNeedsSeparateAcknowledgement() async
@MainActor func testFailureKeepsOperationAndStderrVisible() async
```

The fingerprint must contain source SHA-256, target SHA-256, selected groups, selected `system`, and CEC acknowledgement; it must not be a timestamp or a path-only string.

- [ ] **Step 2: Confirm the intended RED state.**

Run:

```bash
rtk proxy swift test --package-path apps/mh3g-save-converter-macos --filter ConversionWorkflowTests
```

Expected: package or `ConversionWorkflow` symbols are missing.

- [ ] **Step 3: Implement a testable async argv-only command client.**

Create these boundary types:

```swift
struct ConverterCommand: Sendable, Equatable {
    let executable: URL
    let arguments: [String]
}

struct ConverterCommandResult: Sendable {
    let exitCode: Int32
    let stdout: Data
    let stderr: Data
}

protocol ConverterCommandExecuting: Sendable {
    func run(_ command: ConverterCommand) async throws -> ConverterCommandResult
}
```

`ConverterCommandClient` must set `Process.executableURL` and `Process.arguments`; it must never invoke `/bin/sh`, concatenate an argument string, call `waitUntilExit()` on the main actor, or wait for exit before concurrently draining both pipes. Treat nonzero exit as `ConverterCommandError.failed(exitCode:stderr:)`, retain raw stderr, and decode only valid JSON after a successful exit.

Implement `ConversionWorkflow` as `@MainActor final class`, injecting the protocol above. Its states are `.input`, `.componentSelection`, `.dryRun`, `.writing`, `.success`, `.failure`; `canWrite` is true only for a valid, current fingerprint with a successful dry-run and no active operation. `writeCore`, `writeSystem`, `stageExtras`, `installExtraGroups`, and `writeCEC` must append the corresponding dry-run `--expected-*sha256` values to the argv array, so the backend rejects a post-dry-run source or target change. The workflow must expose these explicit commands and `rollback` rather than one broad directory-copy operation.

- [ ] **Step 4: Run green Swift tests and inspect no shell use.**

Run:

```bash
rtk proxy swift test --package-path apps/mh3g-save-converter-macos
rtk grep -nE '(/bin/sh|waitUntilExit\(\)|arguments\.joined)' apps/mh3g-save-converter-macos/Sources/ConverterPresentation || true
```

Expected: all tests pass and the grep command has no output.

- [ ] **Step 5: Commit the macOS workflow core.**

```bash
rtk git add apps/mh3g-save-converter-macos/Package.swift apps/mh3g-save-converter-macos/Sources/ConverterPresentation apps/mh3g-save-converter-macos/Tests/ConverterPresentationTests
rtk git commit -m 'feat(macos): add converter workflow core'
```

### Task 6: Build the macOS SwiftUI workbench and bilingual accessibility layer

**Files:**
- Create: `apps/mh3g-save-converter-macos/Sources/MH3GSaveConverterMac/MH3GSaveConverterApp.swift`
- Create: `apps/mh3g-save-converter-macos/Sources/MH3GSaveConverterMac/{ConversionWorkbenchView.swift,InputInspectionView.swift,ComponentSelectionView.swift,DryRunView.swift,WriteRollbackView.swift,ExperimentalCECView.swift,SettingsView.swift,SceneArtworkView.swift,OpenPanel.swift}`
- Create: `apps/mh3g-save-converter-macos/Sources/MH3GSaveConverterMac/Resources/Localizable.xcstrings`
- Create: `apps/mh3g-save-converter-macos/Tests/ConverterPresentationTests/LocalizationTests.swift`

- [ ] **Step 1: Add failing localization and accessibility tests.**

Test that the shipped strings catalog contains both `zh-Hans` and `en`, that every sidebar phase has an accessibility label, and that an explicit locale override is persisted and replaces the system locale for newly rendered text.

```swift
func testStringCatalogContainsChineseAndEnglish() throws
func testWorkflowPhaseLabelsAreAccessible() throws
@MainActor func testLocaleOverrideChangesDisplayedLocale() async
```

- [ ] **Step 2: Run the test to establish RED.**

Run:

```bash
rtk proxy swift test --package-path apps/mh3g-save-converter-macos --filter LocalizationTests
```

Expected: failure because UI resources and labels do not exist.

- [ ] **Step 3: Implement the native window UI.**

Use `@main struct MH3GSaveConverterApp: App` and one `WindowGroup`; do not set `LSUIElement`. `ConversionWorkbenchView` uses `NavigationSplitView` with these exact rows: `输入与检查`, `组件选择`, `Dry Run`, `写入与回滚`, then `转换历史`, `实验性 CEC`, `设置`.

Use `NSOpenPanel` only for an explicitly requested source file, target file, ExtData directory, or CEC directory. Resolve a selected directory only into documented file names and show the resolved list; never recursively scan, infer an MLC root, or silently select components.

For the visual layer, use each generated `SceneArtworkView` image behind a low-contrast top region and place forms/tables on one readable system material. Use system semantic colours, `ContentUnavailableView`, `LabeledContent`, `Table`, `ProgressView`, `confirmationDialog`, `accessibilityLabel`, keyboard focus, Dynamic Type and the system font. Use `@Environment(\\.accessibilityReduceMotion)` to replace stage transitions with a short opacity change; do not block input with animation. Set `AppStorage("mh3g.converter.localeOverride")` to `system`, `zh-Hans`, or `en`, and inject `.environment(\\.locale, selectedLocale)`.

The write dialog must enumerate count, exact target directory, group names, backup count, manifest path and CEC experimental status. Its primary action calls only the current workflow operation; no visual progress is advanced without a real command result. The CEC view starts collapsed and disabled until the user selects the exact CEC mailbox and separately acknowledges its experimental state.

- [ ] **Step 4: Verify the UI layer.**

Run:

```bash
rtk proxy swift test --package-path apps/mh3g-save-converter-macos
rtk proxy swift build -c release --package-path apps/mh3g-save-converter-macos
```

Expected: all unit tests and the release executable build successfully.

- [ ] **Step 5: Commit the SwiftUI workbench.**

```bash
rtk git add apps/mh3g-save-converter-macos/Sources/MH3GSaveConverterMac apps/mh3g-save-converter-macos/Tests/ConverterPresentationTests
rtk git commit -m 'feat(macos): add native converter workbench'
```

### Task 7: Package and safely exercise the macOS app

**Files:**
- Create: `scripts/build-mh3g-save-converter-macos-app.sh`
- Create: `scripts/package-mh3g-save-converter-macos.sh`
- Create: `scripts/mh3g-save-converter-macos-smoke.sh`
- Create: `.github/workflows/mh3g-converter-ui-macos.yml`

- [ ] **Step 1: Write the packaging smoke test first.**

`mh3g-save-converter-macos-smoke.sh` must create a `mktemp -d` fixture containing only a synthetic `user2` source, synthetic Cemu `user2` target and valid-extdata header fixtures. It must invoke the bundled CLI for inspect, dry-run, write, `install-extras` dry-run, and rollback; it must assert source SHA-256 does not change and remove the temporary root in a `trap`.

Run it before creating the build scripts:

```bash
rtk proxy bash scripts/mh3g-save-converter-macos-smoke.sh
```

Expected: failure because the app package and bundled converter do not exist.

- [ ] **Step 2: Implement a normal foreground `.app` package.**

The build script must run `cargo build --locked --release -p mh3g-save-convert`, `swift build -c release --package-path apps/mh3g-save-converter-macos`, create `artifacts/mh3g-save-converter-macos/MH3G Save Converter.app`, copy both executables into `Contents/MacOS`, copy `MH3GSaveConverter.icns`, and write an `Info.plist` with:

```xml
<key>CFBundleExecutable</key><string>MH3GSaveConverterMac</string>
<key>CFBundleIdentifier</key><string>org.mhtoolkit.mh3g-save-converter</string>
<key>CFBundleIconFile</key><string>MH3GSaveConverter</string>
<key>LSMinimumSystemVersion</key><string>15.0</string>
```

Do not include `LSUIElement`. Run `plutil -lint`, `codesign --force --sign - --timestamp=none`, `codesign --verify --deep --strict`, bundled `mh3g-save-convert --help`, and an app `--diagnostics` mode that reports UI and bundled CLI version without opening a window.

The package script must ZIP the app plus bilingual guide and SHA-256 checksum, then extract it into a clean temporary directory and repeat the diagnostics and CLI smoke checks.

- [ ] **Step 3: Make the smoke suite green and perform a foreground launch check.**

Run:

```bash
rtk proxy bash scripts/build-mh3g-save-converter-macos-app.sh
rtk proxy bash scripts/mh3g-save-converter-macos-smoke.sh
rtk proxy bash scripts/package-mh3g-save-converter-macos.sh
rtk proxy open -na 'artifacts/mh3g-save-converter-macos/MH3G Save Converter.app'
```

Expected: package and synthetic workflow tests pass. The last command opens a normal Dock-visible window; immediately quit it after visual inspection without launching Cemu or selecting a real MLC.

- [ ] **Step 4: Commit macOS delivery automation.**

```bash
rtk git add scripts/build-mh3g-save-converter-macos-app.sh scripts/package-mh3g-save-converter-macos.sh scripts/mh3g-save-converter-macos-smoke.sh .github/workflows/mh3g-converter-ui-macos.yml
rtk git commit -m 'build(macos): package converter workbench'
```

### Task 8: Build a testable Windows workflow core before WinUI controls

**Files:**
- Create: `apps/mh3g-save-converter-windows/MH3GSaveConverter.sln`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter.Core/MH3GSaveConverter.Core.csproj`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter.Core/{ConverterCommand.cs,ConverterCommandClient.cs,ConversionWorkflow.cs,ConversionPlan.cs}`
- Create: `apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/MH3GSaveConverter.Tests.csproj`
- Create: `apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/ConversionWorkflowTests.cs`

- [ ] **Step 1: Write failing .NET workflow tests.**

Use xUnit and a fake `IConverterCommandRunner`. Write the same behavioural contract as macOS:

```csharp
[Fact] public async Task Write_is_disabled_until_a_current_dry_run_succeeds();
[Fact] public async Task Source_hash_change_invalidates_authorization();
[Fact] public async Task Extra_group_selection_is_expanded_only_as_an_atomic_group();
[Fact] public async Task CEC_write_requires_experimental_acknowledgement();
[Fact] public async Task Process_failure_surfaces_complete_stderr_without_success_state();
```

- [ ] **Step 2: Confirm RED on the portable test project.**

Run:

```bash
rtk proxy dotnet test apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/MH3GSaveConverter.Tests.csproj
```

Expected: failure because the solution and workflow core are missing.

- [ ] **Step 3: Implement the platform-neutral core.**

Define a `ConverterCommand` with `string ExecutablePath` and `IReadOnlyList<string> Arguments`. The only production process implementation must use `ProcessStartInfo.ArgumentList`, `UseShellExecute = false`, redirected standard output/error, `WaitForExitAsync`, and concurrent `ReadToEndAsync` tasks. It must not use `cmd.exe`, PowerShell, `Arguments` string concatenation, or UI-thread blocking waits.

`ConversionWorkflow` must keep a `DryRunFingerprint` made from the real source and target SHA-256 plus selected component groups and CEC acknowledgement. It emits immutable state snapshots for the WinUI layer, exposes a current `CanWrite` predicate, and sends only `convert`, `convert-system`, `convert-extras`, `install-extras`, `convert-cec`, `rollback`, `rollback-extras`, or `rollback-cec` argv arrays. Every write argv includes its matching `--expected-*sha256` values from the dry-run report; a UI-only disabled button is never treated as the concurrency guard.

- [ ] **Step 4: Verify green portable tests.**

Run:

```bash
rtk proxy dotnet test apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/MH3GSaveConverter.Tests.csproj
rtk grep -nE '(cmd\\.exe|powershell|\\.Arguments\s*=)' apps/mh3g-save-converter-windows/src/MH3GSaveConverter.Core || true
```

Expected: tests pass and the grep has no output.

- [ ] **Step 5: Commit the Windows workflow core.**

```bash
rtk git add apps/mh3g-save-converter-windows/MH3GSaveConverter.sln apps/mh3g-save-converter-windows/src/MH3GSaveConverter.Core apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests
rtk git commit -m 'feat(windows): add converter workflow core'
```

### Task 9: Build the WinUI 3 presentation and resource layer

**Files:**
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter/MH3GSaveConverter.csproj`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter/{App.xaml,App.xaml.cs,MainWindow.xaml,MainWindow.xaml.cs}`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter/ViewModels/WorkbenchViewModel.cs`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter/Views/{InputInspectionPage.xaml,ComponentSelectionPage.xaml,DryRunPage.xaml,WriteRollbackPage.xaml,ExperimentalCecPage.xaml,SettingsPage.xaml}`
- Create: `apps/mh3g-save-converter-windows/src/MH3GSaveConverter/Strings/{en-US/Resources.resw,zh-CN/Resources.resw}`
- Modify: `apps/mh3g-save-converter-windows/assets/MH3GSaveConverter.ico`

- [ ] **Step 1: Add failing resource and project-contract checks.**

Create `tests/MH3GSaveConverter.Tests/WinUiContractTests.cs` that loads the `.resw` files and asserts both locales contain `Navigation.Input`, `Navigation.Components`, `Navigation.DryRun`, `Navigation.WriteRollback`, `Navigation.ExperimentalCec`, and `Settings.Language.System`. It must also assert the app project has `TargetFramework` `net8.0-windows10.0.19041.0`, `<UseWinUI>true</UseWinUI>`, and `<WindowsAppSDKSelfContained>true</WindowsAppSDKSelfContained>`.

- [ ] **Step 2: Establish RED.**

Run:

```bash
rtk proxy dotnet test apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/MH3GSaveConverter.Tests.csproj --filter WinUiContractTests
```

Expected: resource/project files are not present.

- [ ] **Step 3: Implement a platform-native workbench.**

Use a WinUI 3 `NavigationView`, `MicaBackdrop`, native `ContentDialog`, `FileOpenPicker`/`FolderPicker`, `ProgressRing`, `InfoBar`, `TeachingTip`, keyboard navigation and `AutomationProperties.Name`. Bind UI state only to the tested `ConversionWorkflow`. The navigation ordering and semantics must match macOS, while retaining Windows idioms:

```text
输入与检查 / Input & inspect
组件选择 / Components
Dry Run
写入与回滚 / Write & rollback
转换历史 / History
实验性 CEC / Experimental CEC
设置 / Settings
```

Use the generated phase artwork at the top of each page with an opaque/legible content region below it; do not draw a fake macOS sidebar or use a WebView. Persist language override as `system`, `zh-CN`, or `en-US` using `ApplicationData.Current.LocalSettings`, defaulting to the Windows language. The CEC page is hidden behind an explicit experimental acknowledgement. The write `ContentDialog` lists exactly the selected file groups, destination, backups, manifest and CEC state.

- [ ] **Step 4: Build and test on a native Windows runner.**

Run on Windows (locally only if available, otherwise in CI):

```powershell
rtk proxy dotnet test apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/MH3GSaveConverter.Tests.csproj
rtk proxy dotnet build apps/mh3g-save-converter-windows/src/MH3GSaveConverter/MH3GSaveConverter.csproj -c Release -p:Platform=x64
```

Expected: resource tests and the x64 WinUI project build pass. A Wine/GameHub/CrossOver result is auxiliary evidence only; it must not replace this native Windows build.

- [ ] **Step 5: Commit WinUI presentation.**

```bash
rtk git add apps/mh3g-save-converter-windows/src/MH3GSaveConverter apps/mh3g-save-converter-windows/tests/MH3GSaveConverter.Tests/WinUiContractTests.cs
rtk git commit -m 'feat(windows): add native converter workbench'
```

### Task 10: Package Windows x64 and wire target-native CI

**Files:**
- Create: `scripts/package-mh3g-save-converter-windows.ps1`
- Create: `scripts/mh3g-save-converter-windows-smoke.ps1`
- Create: `.github/workflows/mh3g-converter-ui-windows.yml`
- Modify: `README.md`
- Modify: `README.zh-CN.md`

- [ ] **Step 1: Write a failing package verification script.**

The smoke script must require an extracted package containing exactly `MH3GSaveConverter.exe`, `mh3g-save-convert.exe`, `MH3GSaveConverter.ico`, `README-Windows.txt`, and `checksums.sha256`. It must invoke the bundled CLI `--help`, invoke `MH3GSaveConverter.exe --diagnostics`, calculate SHA-256 values, and run synthetic source/target dry-run/write/rollback with the bundled CLI. It must not attempt to run a game or Cemu.

Run:

```powershell
rtk proxy pwsh -NoProfile -File scripts/mh3g-save-converter-windows-smoke.ps1
```

Expected: failure because no package exists.

- [ ] **Step 2: Implement reproducible x64 packaging.**

The package script must build the static MSVC CLI with `-C target-feature=+crt-static`, `dotnet publish -c Release -r win-x64 --self-contained true -p:Platform=x64`, copy the exact CLI into the publish directory, produce a ZIP and `checksums.sha256`, then extract to a clean directory and call the smoke script. It must fail if the UI diagnostics report a CLI version different from `mh3g-save-convert --version`.

The Windows workflow uses `windows-2022`, `dtolnay/rust-toolchain@stable` with `x86_64-pc-windows-msvc`, `actions/setup-dotnet` with .NET 8, `dotnet test`, package, extracted smoke and artifact upload. It must cover changed Rust, Windows app, scripts, workflow and bilingual guide paths.

- [ ] **Step 3: Run verification on the supported platform.**

Run in a Windows environment:

```powershell
rtk proxy pwsh -NoProfile -File scripts/package-mh3g-save-converter-windows.ps1
rtk proxy pwsh -NoProfile -File scripts/mh3g-save-converter-windows-smoke.ps1 -PackageRoot artifacts/mh3g-save-converter-windows-x64
```

Expected: an x64 package validates itself and synthetic conversion restores the original target. Record native Windows 11 manual UI validation separately before calling Windows runtime verified.

- [ ] **Step 4: Commit Windows packaging.**

```bash
rtk git add scripts/package-mh3g-save-converter-windows.ps1 scripts/mh3g-save-converter-windows-smoke.ps1 .github/workflows/mh3g-converter-ui-windows.yml README.md README.zh-CN.md
rtk git commit -m 'build(windows): package native converter workbench'
```

### Task 11: Add bilingual operator guides, AI/CLI guidance, and a safe cleanup inventory

**Files:**
- Create: `docs/MH3G_SAVE_CONVERTER_UI_GUIDE.md`
- Create: `docs/MH3G_SAVE_CONVERTER_UI_GUIDE.zh-CN.md`
- Create: `docs/MH3G_SAVE_CONVERTER_AI_CLI_GUIDE.md`
- Create: `docs/MH3G_SAVE_CONVERTER_AI_CLI_GUIDE.zh-CN.md`
- Create: `scripts/mh3g-test-artifact-inventory.py`
- Test: `scripts/mh3g-docs-contract.py`

- [ ] **Step 1: Write failing guide-contract assertions.**

Extend `scripts/mh3g-docs-contract.py` to require both languages to name all exact input shapes: `user1|user2|user3`, optional `system`, `.../extdata/00000000/00000481/user`, optional `.../CEC/00048100`, eight ExtData files, dry-run-before-write, stopped emulator, manifest-bound rollback and experimental CEC. Include a negative assertion that no guide claims ZIP input, automatic MLC discovery or whole-directory overwrite.

- [ ] **Step 2: Confirm RED.**

Run:

```bash
rtk proxy python3 scripts/mh3g-docs-contract.py
```

Expected: failure because the UI/AI guide files do not exist.

- [ ] **Step 3: Write the guides and inventory tool.**

The UI guide documents the four stages, language switching, every optional group, exactly what will and will not be modified, CEC's experimental warning, how to retain a manifest, and how to roll back. The AI/CLI guide gives argv-array examples for `inspect`, `convert --dry-run`, `convert --write`, `convert-extras --write <new staging dir>`, `install-extras --dry-run`, `install-extras --write`, `rollback`, `rollback-extras`, `inspect-cec`, `convert-cec --experimental --dry-run`, and `rollback-cec`; it explicitly prohibits shell interpolation and real-save writes without a verified dry-run.

`mh3g-test-artifact-inventory.py` accepts only `--root <directory> --output <json>`, walks without following symlinks, records relative path, type, byte count, SHA-256 for regular files, a conservative classification, and a `retain` boolean. It must classify original `user#`, `system`, ExtData, CEC, release archives, manifests, backups, and unclassifiable entries as retained. It never calls unlink, rename, copy or write outside the declared output JSON.

- [ ] **Step 4: Verify guides and non-destructive inventory behaviour.**

Run:

```bash
rtk proxy python3 scripts/mh3g-docs-contract.py
rtk proxy python3 scripts/mh3g-test-artifact-inventory.py --root "$(rtk proxy mktemp -d)" --output /tmp/mh3g-inventory.json
rtk proxy python3 -c 'import json; assert isinstance(json.load(open("/tmp/mh3g-inventory.json"))["entries"], list)'
```

Expected: guides pass contract checks and inventory writes only its JSON report.

- [ ] **Step 5: Commit documentation and inventory.**

```bash
rtk git add docs/MH3G_SAVE_CONVERTER_UI_GUIDE.md docs/MH3G_SAVE_CONVERTER_UI_GUIDE.zh-CN.md docs/MH3G_SAVE_CONVERTER_AI_CLI_GUIDE.md docs/MH3G_SAVE_CONVERTER_AI_CLI_GUIDE.zh-CN.md scripts/mh3g-test-artifact-inventory.py scripts/mh3g-docs-contract.py
rtk git commit -m 'docs(mh3g): add native UI and AI CLI guides'
```

### Task 12: Final verification, review, and PR handoff

**Files:**
- Modify only files required by review findings.
- Do not create an Android converter project in this desktop delivery; ADR 0014 records Android as the next separately verified phase after a connected device is available.

- [ ] **Step 1: Run the complete safe verification matrix.**

Run:

```bash
rtk cargo test --locked -p mh3g-save-convert
rtk proxy swift test --package-path apps/mh3g-save-converter-macos
rtk proxy bash scripts/build-mh3g-save-converter-macos-app.sh
rtk proxy bash scripts/mh3g-save-converter-macos-smoke.sh
rtk proxy python3 scripts/verify-mh3g-save-converter-assets.py
rtk proxy python3 scripts/mh3g-docs-contract.py
rtk git diff --check
```

Expected: every command passes without using a real source save, real Cemu MLC or launching Cemu.

- [ ] **Step 2: Obtain Windows-native evidence without blocking on a slow self-hosted runner.**

Push the feature branch, open a pull request, and dispatch the `mh3g-converter-ui-windows.yml` workflow. Read only the first available native Windows job result and artifact metadata; do not wait indefinitely for a self-hosted runner. Record the exact workflow URL, commit SHA, artifact name, SHA-256 and any remaining need for manual Windows 11 UI confirmation.

- [ ] **Step 3: Review scope and quality before merge.**

Review against this checklist:

```text
- Rust backend rejects active or unobservable emulator state on every write and rollback path.
- ExtData card and quest writes/rollbacks are complete-group transactions.
- Both UIs use argument arrays and asynchronous stdout/stderr collection.
- Both UIs keep Write disabled without a current dry-run fingerprint.
- CEC is visibly experimental and off by default.
- No real saves, MLC directories, ROMs, keys or screenshots are committed.
- macOS bundle is a normal foreground app; Windows app uses WinUI 3 rather than Vue/Tauri/WebView.
- Guides are bilingual and enumerate exact inputs and non-effects.
```

- [ ] **Step 4: Open a reviewed PR rather than writing to `main`.**

Run:

```bash
rtk git status --short
rtk git log --oneline origin/main..HEAD
rtk git push -u origin feat/phase1-save-sync
rtk gh pr create --base main --head feat/phase1-save-sync --title 'feat(mh3g): add native save converter workbenches' --body-file /tmp/mh3g-native-ui-pr.md
```

Expected: branch and PR exist; `main` remains changed only through review. Include tested macOS evidence, Windows CI state, explicit Android deferral, and no claim that GameHub/CrossOver is native Windows acceptance.

---

## Plan self-review

- **Spec coverage:** Tasks 1–3 cover the required ADR correction, Windows process gate, CEC gate and ExtData group transaction. Tasks 4–7 cover the selected visual treatment, native macOS workbench, language override, accessibility, app packaging and synthetic verification. Tasks 8–10 cover WinUI 3, not Vue/Tauri, async argv process handling, native x64 packaging and CI. Task 11 covers the bilingual README/AI CLI requirements and safe inventory. Task 12 preserves the PR-only `main` policy and defers Android until hardware is available.
- **No placeholder scan:** This plan uses no unresolved task markers, no silent broad directory writes, and gives concrete file paths, APIs, tests, commands and expected outcomes for every implementation task.
- **Type consistency:** `ProcessProbe` is the single backend gate; `ExtraGroup`, `ExtraInstallManifest`, `install_extra_groups_with` and `rollback_extra_groups_with` are the names used consistently by Rust and UI tasks. Both UI cores use `ConverterCommand`, `ConverterCommandExecuting`/`IConverterCommandRunner`, `ConversionWorkflow` and a `DryRunFingerprint` with the same authority model.
