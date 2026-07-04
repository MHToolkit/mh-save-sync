# ADR 0009: Visible, constrained platform background execution

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

macOS uses a menu-bar app plus a narrowly scoped helper/LaunchAgent. FSEvents
marks configured roots dirty and process lifecycle closes sessions. No
high-frequency full-disk polling is allowed.

Android uses persisted SAF document-tree grants. An active emulator session may
run a user-visible foreground service. Normal exit schedules a
`OneTimeWorkRequest`; reconciliation uses `PeriodicWorkRequest` at a minimum
15-minute-class interval. Users may require unmetered network, battery-not-low
and/or charging.

If permissions, notification permission, background scheduling or storage
access is unavailable, the client displays a durable degraded/error state. It
does not claim continuous protection and does not request root or Accessibility.
Remote availability never blocks local play.

