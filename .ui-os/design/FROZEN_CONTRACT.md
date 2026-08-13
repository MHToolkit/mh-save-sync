# Frozen Windows UI contract v0.1 — Guided Fluent workspace

Status: **frozen for candidate implementation**. Research receipt: `../research/reviews/windows-winui3-standard-20260813-review.json` (`pass`). Material IA, visual-language, state-meaning, or motion-intent changes require a new contract revision and fresh review; implementation must not silently accept a new baseline.

## Shared kernel invariants

1. The product task remains `source / read-only current reference / output → Inspect → optional configure or skip → Dry Run → Confirm Write → Result / Rollback`.
2. New conversion uses source and output. Repair uses three independent roles: original 3DS source, current Wii U/Cemu reference (read-only), and separate output.
3. App navigation has exactly four grouped destinations: Convert, History, Experimental CEC, Settings.
4. Convert is one bounded four-step task: Input, Optional Data, Dry Run, Write/Result. Progress is not a second app navigator.
5. Each current task surface has one page title, one scoped state presentation, and one primary action.
6. The primary action remains in the first viewport at 1,120×760 and 920×600. If disabled, one adjacent plain-language reason and one executable Fix appear in the same footer host.
7. Optional data is opt-in and independently authorized. An incomplete optional domain blocks only its own Dry Run/write. It never blocks core Inspect, core Dry Run, or core write.
8. History and Settings never render `optional.missing-path` or another conversion blocker.
9. Experimental CEC remains independent, experimental, acknowledged, dry-run authorized, hash-bound, confirmed, backed up, manifested and reversible.
10. Rust CLI argv, JSON status, fingerprints, expected hashes, output-absence intent, transaction manifests, backups, confirmations and rollback are authoritative and fail closed.
11. Fixtures cannot invoke the CLI, read real saves, create targets, or claim a real write. Synthetic success is clearly marked as preview-only evidence.

## Windows profile mapping

- `NavigationView` owns application routes. Its native back/navigation semantics are not used to skip transaction gates.
- Use WinUI Button, ToggleSwitch, CheckBox, ComboBox, TextBox, InfoBar, Expander, ProgressBar, ContentDialog, FileOpenPicker and FolderPicker.
- Controls use WinUI `SymbolIcon`/licensed vector glyphs plus localized labels; interaction icons are never rasterized text.
- Work surfaces use theme resources and an explicit High Contrast resource dictionary. Hard-coded light-only foreground/background colors are forbidden.
- Default width is 1,120 and minimum is 920×600. The active content reflows to one column before clipping.
- XAML/task order defines logical Tab order. Primary and local Fix controls expose stable AutomationId and Name; scoped state uses one polite live region.
- Paths may visually ellipsize only while UI Automation retains the complete value and role.

## Information hierarchy and plain copy

1. Page title and one-sentence task purpose.
2. Current-step progress and scoped readiness.
3. Required controls and selected-path summaries.
4. Current primary command plus adjacent reason/Fix.
5. Technical details disclosure for CLI path, hashes, reports, version detection and manifests.

Preferred role labels:

- `3DS source`
- `Current Wii U reference (read-only)`
- `Output`
- `Optional data`
- `Skip optional data`
- `Dry Run checks these exact files without writing.`

## Stable semantic IDs

- Routes: `mh3g.converter.windows.navigation.{convert,history,experimentalCEC,settings}`.
- Titles: `mh3g.converter.windows.page.{input,optionals,dryRun,writeResult,history,experimentalCEC,settings}.title`.
- Primary actions: `mh3g.converter.windows.action.{inspect,continueOptionals,runDryRun,confirmWrite,startConversion}`.
- Local actions: `mh3g.converter.windows.action.{fixOptional,skipOptional,chooseSource,chooseCurrent,chooseOutput,rollback}`.
- State: `mh3g.converter.windows.state.{inputMissing,optionalMissing,optionalSkipped,dryRunReady,dryRunBlocked,writeAuthorized,running,success,failure,historyEmpty}`.
- Path rows: `mh3g.converter.windows.path.{source,current,output,systemSource,systemTarget,extdataSource,extdataTarget,cecSource,cecTarget}`.

## Deterministic fixture contract

Launch fixtures are opt-in through explicit `--ui-fixture <id>` / `MH3G_UI_FIXTURE` parsing before the window is created:

- `first-run`
- `input.empty`
- `components.optional-missing`
- `components.optional-skipped`
- `dry-run.ready`
- `dry-run.blocked`
- `write.authorized`
- `write.confirmation`
- `conversion.success`
- `conversion.failure`
- `history.empty`
- `history.result`

Every fixture uses a fixed seed and synthetic `C:\UIFixture\...` presentation paths. No fixture may call `ConverterCliClient` or `FileFingerprintService`, and no destructive dialog fixture may accept its primary action.

## Motion contract

| Seam | Purpose / frequency | Windows behavior | Reduced motion | Evidence |
| --- | --- | --- | --- | --- |
| Inspect/Dry Run/Write starts | Immediate receipt; occasional | CTA frame remains fixed; local text/state changes immediately and native indeterminate `ProgressBar` appears | Same text and progress semantics; no custom motion | state trace + normal/reduced terminal frames |
| Optional enabled | Relate toggle to dependent fields; occasional | Short interruptible local opacity/content transition, no whole-page movement | Immediate reveal/collapse | state trace + focus order |
| Picker returns | Confirm cause/effect; frequent | Path row updates in place and retains/reclaims focus; no toast | Immediate update | interaction trace + UIA value |
| Blocked → ready → authorized/result | Clarify state progression; occasional | Fixed footer host changes icon/text/surface over one short native transition | Immediate static replacement | transition trace + final screenshots |
| Success | Rare completion | One bounded check/status transition; never loops | Immediate check/status | bounded trace + terminal frame |

Custom motion is permitted only when `Windows.UI.ViewManagement.UISettings.AnimationsEnabled` is true. No page slide, navigation delay, form stagger, hover lift/scale, shimmer, confetti, failure shake, flash, or looping empty-state motion.

## Required responsive/state matrix

Critical coverage on a native Windows runner:

- 1,120×760 default and 920×600 minimum;
- Light, Dark, High Contrast;
- English and zh-Hans;
- 100%, 150%, and critical 200% text scaling cells;
- pointer and full keyboard path;
- normal and disabled animation preference;
- input empty, optional missing/skipped, Dry Run ready/blocked/running, write authorized/confirmation/running, success/failure, history empty/result, recovery.

## Acceptance gates

- 0 duplicate primary navigation.
- 0 optional blocker instances on History or Settings.
- 0 critical clipping, overlap, inaccessible action, or focus trap in required native Windows cells.
- One primary action per current task surface.
- Disabled primary action has one adjacent reason and executable Fix.
- UI Automation identifiers are unique and stable.
- Accessibility critical findings: 0.
- Motion normal/reduced traces preserve identical state meaning and stable primary-action geometry.
- Machine, independent AI, Windows runtime, and human/task verdicts remain separate. Human stays pending until a real target user completes the task.

