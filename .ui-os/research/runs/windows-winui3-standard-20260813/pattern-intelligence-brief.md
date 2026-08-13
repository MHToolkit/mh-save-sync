# Windows WinUI 3 Pattern Intelligence Brief

Run: `windows-winui3-standard-20260813`  
Target: Windows desktop, WinUI 3 / Windows App SDK 1.8, pointer + keyboard + UI Automation  
Scope: MH3G Save Converter only

## Product truth inspected

The existing Windows app is one long `ScrollViewer` containing a decorative hero, four stage labels, repeated global `InfoBar` next-step prompts, core paths, optional System/ExtData/CEC controls, raw report text, history, and rollback. It uses WinUI native controls but also hard-coded light surfaces and a global status badge. The result makes application navigation, ordered conversion state, optional transactions, and recovery compete in one hierarchy.

The shared kernel must remain unchanged:

`source / current reference / output → Inspect → Optional Data or Skip → Dry Run → Confirm Write → Result / Rollback`

## Live source lanes

| Source ID | Lane | Current task-fit observation |
|---|---|---|
| `ms-navigationview` | Windows platform authority | `NavigationView` is the native adaptive container for top-level destinations; keyboard behavior and High Contrast resources are first-class requirements. |
| `ms-file-picker` | WinUI implementation | Windows App SDK 1.8 supplies `FileOpenPicker` and `FolderPicker` constructed with `WindowId`, returning selected paths through the familiar Windows surface. |
| `ms-progress-controls` | WinUI implementation | `ProgressBar` is suitable for nonmodal work, while `ProgressRing` communicates a wait that can block interaction; explanatory text may still be necessary. |
| `ms-accessibility-overview` | Accessibility authority | XAML controls expose keyboard and UI Automation behavior, but focus order, names, text scaling, High Contrast, and assistive-technology output must be explicitly tested. |
| `fluent-motion` | Windows motion authority | Motion should express relationship and feedback, not decorate operational surfaces or delay commands. |
| `ms-ui-settings-motion` | Windows implementation | `UISettings.AnimationsEnabled` exposes the user's animation preference and must gate custom transitions. |
| `github-desktop-onboarding` | Shipped desktop flow | Contextual next steps address the empty-state question “what now?” without requiring users to infer the entire workflow. This is historical flow evidence, not current visual authority. |
| `balena-etcher` | Shipped guarded utility | A source/target/write utility foregrounds safety, verification, and recovery. Only the principle is reusable; its brand, copy, and composition are not. |

All eight public sources were fetched live by the bundled UIOS fetcher on 2026-08-13 and are stored with HTTP receipts and content hashes in this run.

## Direction A — Guided native workflow

- One WinUI `NavigationView` for Convert, History, Advanced, and Settings.
- Inside Convert, one bounded stage is active at a time; the progress cue is state, not another navigator.
- One title, one scoped state, one primary action, and one adjacent reason/Fix per task surface.
- Optional Data starts collapsed and may be skipped. Enabling an optional transaction reveals only its dependent paths and safety copy.
- File/folder commands use Windows App SDK pickers; returning selection updates a local selected-path row and preserves focus.
- Best fit: first-time users, minimum-window legibility, and plain next-step guidance.

## Direction B — Compact operation canvas

- Same application `NavigationView`, but Convert is a compact source/current/output summary with a stable command footer.
- Inspect and recovery details expand at the affected row; hashes, fingerprints, manifests, and raw reports remain progressive disclosure.
- Nonmodal Inspect/Dry Run use nearby text plus `ProgressBar`; only the actual write boundary uses confirmation and any blocking state.
- Best fit: returning users who compare inputs and repeat repairs frequently.

Both directions share the product kernel, semantic actions, safety gates, backup/manifest/rollback behavior, and stable automation IDs. They differ structurally rather than by palette.

## Windows-native mapping

- **Navigation:** `NavigationView` only for application destinations; no second sidebar and no full-page slide choreography.
- **Selection:** Windows App SDK 1.8 `FileOpenPicker` / `FolderPicker`, followed by a local path confirmation row.
- **Status and recovery:** scoped `InfoBar`, text, icon, enabled state, and nearby Fix; no global optional blocker.
- **Progress:** native `ProgressBar` for nonmodal unknown-duration work; `ProgressRing` only where the affected surface truly must wait.
- **Confirmation:** `ContentDialog` with explicit source/current/output summary, risk, cancel path, and focus return.
- **Typography and density:** WinUI theme typography and desktop control density; stack to one column before clipping at the minimum window.
- **Icons:** WinUI `SymbolIcon` or licensed project vector assets, paired with localized labels for primary/destructive actions.
- **Surfaces:** theme resources for Light, Dark, and High Contrast; remove hard-coded light-only fills that bypass resource lookup.

## Motion contract input

1. **Async command receipt:** keep the CTA rectangle fixed; immediately change its local status text and show native progress.
2. **Optional reveal:** one short, interruptible local content/opacity transition; never animate the entire `ScrollViewer`.
3. **Picker return:** local confirmation only; no toast or page motion required.
4. **Blocked → ready → authorized/result:** short scoped state morph with persistent text/icon semantics.
5. **Success:** at most one bounded completion transition; no confetti, looping mark, or progress shimmer.

When `UISettings.AnimationsEnabled == false`, custom translation, scale, and crossfade are removed. The same state is conveyed immediately through accessible text, icon, enabled state, focus, and UI Automation properties.

## Accessibility fallback

- DOM/XAML order is the logical Tab order; Enter/Space activates buttons and Escape cancels confirmation.
- Stable `AutomationId` and Name identify route title, selected paths, primary action, blocked reason, Fix, status, manifest, and rollback.
- Use only one polite live region for the affected asynchronous operation; repeated global announcements are rejected.
- Selected paths may visually ellipsize only if UI Automation retains the complete value.
- Copy and controls must reflow for English, Simplified Chinese, increased text scale, and the minimum window.
- Semantic status never depends on color or motion; theme resources and a High Contrast dictionary override custom surfaces.

## Delete / merge implications for design

- Delete the decorative hero and stage artwork from the operational hierarchy.
- Merge repeated next-step `InfoBar` prompts into one stage-local status/recovery presentation.
- Remove the global readiness badge when the same state is already present in the active task footer.
- Move raw hashes, manifest details, and report payloads behind disclosures; keep critical write identity visible in confirmation.
- Isolate optional System, ExtData, and CEC transactions from the core blocker model.
- Keep History and Settings free of conversion readiness errors.

## Rejected candidates

- Card-heavy single-page dashboard with decorative artwork and duplicated stage/status UI.
- Full-window spinner for every picker, Inspect, Dry Run, or optional transaction.
- Page slides, form stagger, hover lift, looping empty visuals, failure shake, or success confetti.
- Treating optional System/ExtData/CEC data as a prerequisite for core conversion.
- Copying balenaEtcher or GitHub Desktop branding, wording, composition, imagery, or physical tokens.

## Copy, asset, and license constraints

- Microsoft and GitHub pages are linked reference only; no copy or screenshots are imported.
- The balenaEtcher repository is Apache-2.0, but this run imports no code, image, layout, or wording from it.
- Control icons must come from WinUI symbols or an already licensed project vector family. No rasterized text or generated control icons.
- External product language is paraphrased into MH Toolkit's safety semantics; optional must never become required, and advisory must never become error.

## Limitations

- Public shipped-product evidence was fetched live, but no new interactive Windows recording or paid flow-database export was captured.
- GitHub Desktop onboarding evidence is historical and supports only the contextual-next-step principle.
- balenaEtcher's fetched repository page does not contain a complete adjacent-state screenshot set.
- Runtime fidelity, minimum-window layout, focus, UI Automation tree, High Contrast, text scaling, and motion remain implementation/capture/verification work on a real Windows runner.

