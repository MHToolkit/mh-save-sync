# Windows Standard design directions

Both directions consume the independently reviewed task-time research run `windows-winui3-standard-20260813` and preserve the same shared product kernel, action identities, state scopes, confirmation, fingerprints, backup, manifest, and rollback behavior. They differ in structure, not palette.

## Direction A — Guided Fluent workspace (selected)

**Shape:** one WinUI `NavigationView` owns application destinations. Convert owns a bounded four-step task whose current step is rendered in one work surface with a persistent command footer.

```text
NavigationView          Convert content
Conversion              Step 1 of 4 — Input
  Convert save          [mode] [slot]
Records                 [3DS source selected-path row]
  History               [current reference in Repair]
Advanced                [output selected-path row]
  Experimental CEC      [technical details disclosure]
Application             --------------------------------
  Settings              status/reason + Fix    [Inspect]
```

### Full flow

1. **Input** — choose source, optional read-only current reference, and output; Inspect is the sole primary action.
2. **Optional Data** — all domains are off by default. Enabling a domain reveals only its required paths and independent actions. **Skip optional data** removes only optional intent; **Continue to Dry Run** never requires optional configuration.
3. **Dry Run** — a Ready/Needs action list explains the exact core transaction. The sole primary action is **Run Dry Run**. A blocker sits in the footer with one local Fix.
4. **Write / Result** — authorized fingerprints lead to native `ContentDialog` confirmation. Running feedback stays local with stable CTA geometry. Success/failure shows report and rollback without false success.
5. **History** — current-session history only; empty state offers **Start conversion** and never hosts conversion blockers.
6. **Experimental CEC** — isolated advanced route with its own acknowledgement and transaction sequence.
7. **Settings** — language, CLI fallback, update checking and accessibility/motion diagnostics; no conversion blocker.

### Expert path

- Windows standard Tab/Shift+Tab order follows task order.
- `Alt` access keys are attached to frequent route/primary commands where WinUI supports them.
- Enter/Space activates focused commands; Escape cancels confirmation and returns focus to the invoker.
- No keyboard shortcut bypasses Inspect, Dry Run, expected-hash binding, confirmation, or rollback evidence.

### Why selected

- Most legible at the 920×600 minimum and at increased text scale.
- Separates application navigation from ordered transaction progress.
- Makes the disabled reason, Fix, and current primary action one stable local unit.
- Keeps first-time guidance while progressive disclosure protects expert density.
- Maps directly to native `NavigationView`, `InfoBar`, `Expander`, picker, progress and dialog primitives.

## Direction B — Compact operation canvas (rejected)

**Shape:** the same app `NavigationView`, but Convert presents a persistent source/current/output comparison canvas with a right-side transaction inspector and collapsible task sections.

```text
NavigationView     Source / current / output      Transaction inspector
Convert            [path summary rows]            status + next action
History            [Inspect details]              Dry Run evidence
CEC                [Optional details]             Write / rollback
Settings
```

### Full flow

- Users can inspect or replace any selected input without moving between steps.
- Optional System/ExtData attach beneath their related source/target row.
- Dry Run, write confirmation, result and recovery stay in the transaction inspector.
- History may reopen a previous session result into the inspector without mutating inputs.

### Why rejected

- At minimum width or 150–200% text scale the comparison and inspector compete, forcing either horizontal compression or a long stacked canvas.
- First-time users must understand the source/current/output topology before seeing a single next action.
- Persistent transaction inspector risks duplicating state already present at the affected input or result surface.
- It is faster for expert repeated repair, but that efficiency does not outweigh weaker first-run clarity for this Pilot.

## Decision matrix

| Criterion | A Guided Fluent workspace | B Compact operation canvas |
| --- | ---: | ---: |
| Five-second task clarity | 5 | 3 |
| Information hierarchy | 5 | 4 |
| Minimum-window behavior | 5 | 3 |
| Windows native fit | 5 | 4 |
| Keyboard/focus predictability | 5 | 4 |
| Text scaling / zh-Hans | 5 | 3 |
| Scoped recovery | 5 | 4 |
| Expert repeat efficiency | 4 | 5 |
| Cross-screen consistency | 5 | 4 |

