# Windows UI Quality OS discovery

## Verified repository baseline

- Repository: `MHToolkit/mh-save-sync`.
- Isolated worktree: `/Volumes/GameHub/Development/Games/mh-save-sync-windows-ui-quality-os`.
- Feature branch: `feat/mh3g-windows-ui-quality-os`.
- Baseline: `origin/main` at `b3d58f70aa713b5e7f0ba7397d745b7166925547` (`v0.0.18`).
- Target stack: unpackaged **WinUI 3**, .NET 8, Windows App SDK `1.8.260710003`, Windows 10 1809 minimum, x64 self-contained packaging.
- Backend boundary: the existing C# presentation invokes the bundled Rust CLI as independent argv elements. Rust remains authoritative for inspection, dry-run authorization, expected hashes, transactional write, manifest, backup, and rollback.

## Runtime baseline status

A downloaded portable executable exists at `/Users/vincentadamnemessis/Downloads/MH3GSaveConverter-Portable-x64.exe`, but it is an older, provenance-uncertain artifact (timestamp 2026-08-01; embedded assembly version `1.0.0.0`) and is not tied to the current `v0.0.18` source hash. This macOS host has no active Windows VM or native Windows runner. Previous GameHub execution was already reported unusable and is not repeated.

Therefore **current Windows runtime baseline screenshots, UI Automation tree, focus traversal, High Contrast, text scaling, and animation evidence are BLOCKED/UNVERIFIED**. The Pilot may improve source, fixtures, source-level gates and Windows CI capture support here, but must not label cross-compilation or static XAML inspection as Windows runtime proof.

## Existing surface inventory

The current app is one 1,240×900 `ScrollViewer` containing the entire product:

- fixed top brand/status/language strip;
- a large illustrated hero and a four-item visual stage strip;
- five post-operation `InfoBar` continuations;
- source/current/output/CLI controls;
- Inspect, progress inspection, event inspection, Dry Run, Write, rollback;
- shared `system`, ExtData, CEC, latest report, and operation history;
- 43 buttons, 16 text boxes, 8 InfoBars, 104 text blocks, 28 hard-coded ARGB colors;
- no `NavigationView`, no `Expander`, no stable `AutomationId`, and no deterministic UI fixture launch contract.

The `StageArtwork` control swaps among five raster images and overlays a second stage presentation. Hard-coded light surfaces and text colors bypass theme and High Contrast resources. A single long surface means task scope, utility scope, history, status, and recovery compete vertically.

## Baseline problems to solve

1. **No primary navigation:** core conversion, optional transactions, CEC, history, report, update, and rollback are stacked into one document.
2. **Repeated state/navigation:** top status, hero stage labels, stage artwork, InfoBars, section copy, and action availability all describe the same progression.
3. **Primary action distance:** Inspect, Dry Run, Write, rollback, and their explanations live in different columns or far-apart vertical regions; the correct next action is not stable at the first screen.
4. **Optional-data contradiction:** `SelectedOptionalDataIsConfigured` currently disables core Dry Run and Write whenever an enabled optional domain is incomplete. This makes “optional” a global core blocker.
5. **Scope leakage risk:** all global InfoBars and optional controls share one root surface, so unrelated history/report/settings content has no structural boundary from optional warnings.
6. **Repair path cognitive load:** source, read-only current reference, output, version detection, and CLI path appear as one uninterrupted technical form.
7. **Technical detail dominance:** CLI path, hashes/reports/manifests, file topology and transaction implementation remain in primary reading order.
8. **Theme/accessibility debt:** hard-coded colors, missing stable AutomationIds, no explicit minimum-size contract, no deterministic keyboard/High Contrast/text-scale fixture coverage.
9. **Decorative displacement:** the hero and stage artwork consume the highest-value area but do not select a step, fix a blocker, or authorize a transaction.
10. **No causal motion contract:** stage artwork transitions exist, but async action acknowledgement, local dependency reveal, selected-path confirmation, and reduced-motion equivalents are undefined.

## Shared product kernel

The Windows profile must preserve the already established cross-client task semantics:

`Select source / read-only current reference / output → Inspect → configure or skip Optional Data → Dry Run → Confirm Write → Result / Rollback`

- New conversion omits the current-reference input.
- Repair keeps source, current reference, and output as three independent roles.
- Optional transactions are opt-in and independently authorized; skipping them never weakens or blocks core conversion.
- Experimental CEC remains an advanced, independently acknowledged transaction.
- No fixture may execute the CLI, touch a real save, or report a synthetic write as real success.

## Delete / merge before adding

| Delete or merge | Windows destination / reason |
| --- | --- |
| Large hero and `StageArtwork` | Remove from operational flow. Keep only a compact product mark and, if useful, one small non-interactive empty-state vector. |
| Four visual stage labels | Replace with one bounded step indicator inside Convert; it communicates location but is not a second app navigator. |
| Top status badge and five global continuation InfoBars | Render state once at the narrowest affected surface; put the next action in a persistent task footer. |
| One giant ScrollViewer | Use a native grouped `NavigationView`: Convert, History, Experimental CEC, Settings. |
| Dry Run/Write side card plus remote reasons | One current-step surface; disabled CTA has one adjacent reason and executable Fix. |
| Always-visible CLI path | Move to Settings/technical disclosure; bundled sidecar remains the default. |
| Raw report/hash/manifest in primary order | Move to `Expander`/dialog technical details except confirmation and recovery-critical evidence. |
| Global optional readiness gate | Scope incomplete optional paths to that component only; core Inspect/Dry Run/Write stay independent. |
| Raster control-like artwork | Use licensed WinUI `SymbolIcon`/font glyphs for interaction and theme-aware vectors for any retained empty-state illustration. |

## Baseline machine evidence

- `scripts/verify-mh3g-save-converter-windows-source.py`: PASS on baseline.
- `cargo test -p mh3g-save-convert`: 220 passed on baseline.
- `cargo clippy -p mh3g-save-convert --all-targets -- -D warnings`: PASS on baseline.
- macOS `dotnet build --no-restore`: expectedly failed because the isolated worktree had no restored Windows-target assets; this is environment evidence, not a product regression.

