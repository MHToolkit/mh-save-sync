# Frozen direction wireframes

These text wireframes define structure and hierarchy, not pixels. Runtime WinUI remains authoritative.

## Input

```text
┌ Navigation ┐  Input & inspection                      Step 1 of 4
│ Convert    │  Choose the exact files for this conversion.
│ History    │
│ CEC        │  Mode [New conversion ▼]  Slot [user2 ▼]
│ Settings   │
└────────────┘  3DS source
                [No source selected                 ] [File] [Folder]

                Output
                [No output selected                 ] [File] [Folder]

                Technical details ▸
                ─────────────────────────────────────────────────
                Choose a source and output to continue.   [Inspect]
```

## Optional missing

```text
Optional data                                      Step 2 of 4
Core conversion does not require optional data.

[on] Shared system
     Source [not selected] [Choose]
     Target [not selected] [Choose]
     ⚠ Shared system needs both paths. [Choose paths]

[off] Guild cards / quests

──────────────────────────────────────────────────────────────
Optional paths affect only this component. [Skip optional data] [Continue]
```

## Dry Run blocked / ready

```text
Dry Run                                            Step 3 of 4
Checks the exact core files without writing.

✓ 3DS source inspected
✓ Output intent inspected
! Original converter version has multiple candidates

──────────────────────────────────────────────────────────────
Choose one detected version, then try again. [Choose version] [Run Dry Run]
```

## Write confirmation / result

```text
Write & result                                     Step 4 of 4
✓ Dry Run authorized these exact files

Source / current / output summary
Technical hash details ▸

──────────────────────────────────────────────────────────────
Authorization is current.                         [Confirm write]

ContentDialog: target + fingerprint + backup + manifest + Cancel / Write

Result: ✓ completed | ✕ failed, report ▸, manifest, Roll back
```

## History empty

```text
History
No transactions in this app session.
Start with an explicit source and output; nothing is scanned automatically.
[Start conversion]
```
