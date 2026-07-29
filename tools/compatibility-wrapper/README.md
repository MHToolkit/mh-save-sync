# Windows legacy compatibility wrapper

> **Only for the legacy `mh3g-save-convert-v0.0.3` converter core.** The core after PR #17 already performs these conversions natively. Do not layer this wrapper on a newer core, or the affected fields will be converted twice.

This directory preserves the tester-provided Windows hotfix build path. The v0.0.3 WinUI package calls `tools/mh3g-save-convert.exe`; this wrapper patches a temporary copy of the input for the missing companion numeric-prefix, guild-card arena, and CEC arena conversions while preserving byte-packed mask mastery state, then delegates all remaining work to `mh3g-save-convert-core.exe`.

## Build on Windows

Install Python 3, Clang, and LLD, then run:

```powershell
cd tools/compatibility-wrapper
./build-windows.ps1
```

The output is `dist/mh3g-save-convert.exe`. Rename the original v0.0.3 converter to `mh3g-save-convert-core.exe` and place both files in the same directory.

## Build on Linux

```bash
cd tools/compatibility-wrapper
./build-linux.sh
```

## Lifecycle and safety

This compatibility path is intentionally not wired into current release packaging. Its generator validates the exact legacy conversion-table shape and fails closed once the repository contains the complete native fix. Current releases must build the Rust converter directly instead of using this wrapper.
