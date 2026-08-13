# Frozen Guided Fluent Direction — Pattern Intelligence Brief

Run: `windows-winui3-frozen-direction-20260813`  
Target: Windows desktop / WinUI 3 / Windows App SDK 1.8

## Decision being revalidated

The frozen direction remains a **Guided Fluent workspace**:

`source / current reference / output → Inspect → Optional Data or Skip → Dry Run → Confirm Write → Result / Rollback`

One WinUI `NavigationView` owns application destinations (`Convert`, `History`, `Advanced`, `Settings`). Inside Convert, a compact progress cue communicates the bounded task state but is not another navigator. Each active state keeps one title, one scoped status, one primary action, and its sole disabled reason/Fix together.

## Live evidence lanes

| Source ID | Lane | Frozen-direction implication |
|---|---|---|
| `ms-nav-frozen` | Windows authority | Use `NavigationView` for top-level destinations; retain keyboard and High Contrast behavior. |
| `ms-picker-frozen` | Implementation | Use Windows App SDK 1.8 file/folder pickers with `WindowId`; confirm the returned path locally and preserve focus. |
| `ms-progress-frozen` | Implementation | Use `ProgressBar` for nonmodal work and `ProgressRing` only for a genuine wait; keep explanatory status text. |
| `ms-a11y-frozen` | Accessibility authority | Verify UI Automation names/roles/values, logical XAML/Tab order, text scaling, and theme-driven High Contrast. |
| `ms-motion-pref-frozen` | Motion implementation | Gate custom transitions with `UISettings.AnimationsEnabled`. |
| `github-next-step-frozen` | Shipped flow | Contextual next steps solve ambiguous first-run/empty states; historical flow principle only. |
| `etcher-guarded-frozen` | Shipped guarded utility | Explicit source, target, write, verification, and recovery support a safety-oriented flow; no brand or layout copying. |

All seven public pages were fetched live in this run on 2026-08-13 with bundled HTTP receipts and content hashes.

## Selected implementation direction

- One native application navigator; no second sidebar and no interactive stage rail.
- Convert reveals one bounded stage at a time while preserving shared kernel and fail-closed semantics.
- Optional System/ExtData/CEC paths remain collapsed until enabled and may be skipped without blocking core Inspect/Dry Run/Write.
- Picker-backed selected-path rows retain complete accessible values even when visual paths ellipsize.
- A stable local command footer holds the primary action, running state, disabled reason, and immediate Fix/recovery.
- Technical hashes, manifest data, and raw reports use progressive disclosure; critical write identity remains visible in confirmation.
- History and Settings never inherit conversion readiness blockers.

## Non-selected comparison

The **compact operation canvas** remains a credible expert pattern: aligned source/current/output review rows above a fixed write action with nearby expandable evidence. It is rejected as the default because it increases simultaneous first-run choices and conflicts with the frozen staged comprehension goal. Its concise review-row treatment may be adapted inside the selected flow without changing navigation or task meaning.

## Windows native profile

- **Navigation:** WinUI `NavigationView` for application destinations only.
- **Selection:** Windows App SDK 1.8 `FileOpenPicker` and `FolderPicker`.
- **Status/recovery:** scoped `InfoBar`, plain text, icon, enabled state, and labeled Fix.
- **Progress:** native `ProgressBar` for nonmodal work; `ProgressRing` only for a true local wait.
- **Confirmation:** `ContentDialog`, explicit risk/identity, Escape/cancel, focus return.
- **Disclosure:** native `Expander` for hashes, manifests, reports, and optional configuration.
- **Icons:** WinUI symbols or existing licensed project vectors, paired with localized text for primary/risky actions.
- **Surfaces:** theme resources for Light, Dark, and High Contrast; no hard-coded light cards.

## Motion and fallback

Adopt only causal seams:

1. Optional enable/skip → short local reveal or state replacement.
2. Blocked → ready → running → result → stable footer content change without moving the command frame.
3. Picker return → local path confirmation; no global toast or page movement.
4. Success → at most one bounded, non-looping completion cue.

No page slide, form stagger, hover lift, looping empty illustration, failure shake, progress shimmer, or confetti. When `UISettings.AnimationsEnabled` is false, custom transitions are removed; persistent text, icon, enabled state, focus, and UI Automation semantics convey the same result immediately.

## Accessibility and copy constraints

- XAML order equals logical Tab order; Enter/Space activates, Escape cancels confirmation, and focus returns to the invoker.
- Stable AutomationIds cover route title, primary action, disabled reason, Fix, status, path picker/value, manifest, and rollback.
- Use one scoped polite live region per active operation; avoid repeated global announcements.
- Reflow before clipping at minimum width and increased text scaling; test English and Simplified Chinese.
- Status uses text and icon in addition to color/motion; High Contrast resources override custom visuals.
- External pages are linked references only. No Microsoft/GitHub/balena brand asset, wording, exact composition, or physical token is copied.
- balenaEtcher is Apache-2.0, but no code or asset is imported. Control icons remain WinUI/project-owned vectors.

## Rejected

- Compact expert canvas as the default first-run surface.
- Decorative hero, global readiness badge, repeated next-step banners, interactive stage rail, or always-visible optional paths.
- Full-window blocking progress for picker, Inspect, or Dry Run.
- Ornament-only motion or animation as the sole success/failure/selection signal.

## Limitations

- Shipped evidence is public release/repository evidence, not a new interactive Windows capture or paid flow database.
- GitHub Desktop evidence is historical and supports only contextual next-step behavior.
- The fetched Etcher page does not provide a complete adjacent-state screenshot set.
- WinUI runtime layout, minimum-window behavior, keyboard focus, UI Automation, High Contrast, text scaling, and motion remain unverified until Windows capture and isolated review.

