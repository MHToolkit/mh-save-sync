# MH Save Sync Cross-Platform Icon System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the approved B3 “encrypted purple” shield/save/check icon across macOS and Android, including dedicated menu-bar and notification glyphs.

**Architecture:** Keep one deterministic geometric source of truth in a Swift/AppKit renderer and generate raster deliverables without network or design-tool dependencies. Use platform-native vector resources for Android adaptive/monochrome/notification icons, an `.icns` bundle for macOS, and a macOS template PDF/PNG for the status item. Build scripts verify every consuming reference so a missing icon fails CI instead of silently falling back.

**Tech Stack:** Swift 6 + AppKit/CoreGraphics, macOS `iconutil`, Android VectorDrawable/adaptive icons, Bash smoke tests, Gradle/aapt.

---

## File Structure

- `design/icon/mh-save-sync-icon.svg`: reviewed canonical vector reference and palette declaration.
- `scripts/generate-app-icons.swift`: deterministic AppKit/CoreGraphics raster generator.
- `scripts/generate-app-icons.sh`: wrapper that creates macOS iconset/ICNS and Android legacy PNGs.
- `scripts/icon-assets-smoke.sh`: resource existence, dimensions, alpha, plist and manifest assertions.
- `apps/macos/Resources/AppIcon/MHSaveSync.icns`: generated app icon.
- `apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png`: generated 36×36 template source.
- `apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml`: adaptive foreground vector.
- `apps/android/app/src/main/res/drawable/ic_launcher_monochrome.xml`: themed icon vector.
- `apps/android/app/src/main/res/drawable/ic_stat_save_sync.xml`: notification alpha-mask vector.
- `apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml`: adaptive icon declaration.
- `apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher_round.xml`: round adaptive declaration.
- `apps/android/app/src/main/res/values/colors.xml`: B3 icon colors.
- `apps/android/app/src/main/res/mipmap-*/ic_launcher*.png`: generated legacy fallbacks.
- `apps/android/app/src/main/AndroidManifest.xml`: launcher icon references.
- `apps/macos/Sources/MHSaveSyncMac/main.swift`: status-item template icon loading.
- `scripts/build-macos-app-bundle.sh`: app icon/resource packaging and plist reference.

### Task 1: Deterministic B3 Source and Generator

**Files:**
- Create: `design/icon/mh-save-sync-icon.svg`
- Create: `scripts/generate-app-icons.swift`
- Create: `scripts/generate-app-icons.sh`
- Create: `scripts/icon-assets-smoke.sh`

- [ ] **Step 1: Write the failing smoke contract**

Create `scripts/icon-assets-smoke.sh` with assertions for:

```bash
test -f design/icon/mh-save-sync-icon.svg
test -f apps/macos/Resources/AppIcon/MHSaveSync.icns
test -f apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  test -f "apps/android/app/src/main/res/mipmap-${density}/ic_launcher.png"
  test -f "apps/android/app/src/main/res/mipmap-${density}/ic_launcher_round.png"
done
sips -g pixelWidth -g pixelHeight apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png \
  | grep -q 'pixelWidth: 36'
```

- [ ] **Step 2: Run the contract and verify RED**

Run: `rtk bash scripts/icon-assets-smoke.sh`  
Expected: non-zero because the approved assets do not exist.

- [ ] **Step 3: Add the canonical vector reference**

Create a 1024×1024 SVG with:

```xml
<linearGradient id="b3" x1="0" y1="0" x2="1" y2="1">
  <stop offset="0" stop-color="#4936B7"/>
  <stop offset="1" stop-color="#9B72F2"/>
</linearGradient>
```

The shield path uses a 68% safe-area footprint; the save slot and verification check use rounded strokes and no text.

- [ ] **Step 4: Implement the deterministic renderer**

`scripts/generate-app-icons.swift` must expose these modes:

```swift
enum RenderMode { case app, menuBar, legacyRound }
func render(size: Int, mode: RenderMode, output: URL) throws
```

Use `NSBitmapImageRep`, `NSGraphicsContext`, `NSBezierPath`, colors `#4936B7`/`#9B72F2`, supersampled drawing, and PNG output. App mode draws the purple gradient background plus white shield/save/check. Menu-bar mode draws only the black alpha glyph on a transparent 36×36 canvas. Legacy-round mode clips the app artwork to a circle.

- [ ] **Step 5: Generate all raster artifacts and `.icns`**

`scripts/generate-app-icons.sh` must:

```bash
swift scripts/generate-app-icons.swift --out-root "$repo_root"
iconutil -c icns apps/macos/Resources/AppIcon/MHSaveSync.iconset \
  -o apps/macos/Resources/AppIcon/MHSaveSync.icns
rm -rf apps/macos/Resources/AppIcon/MHSaveSync.iconset
```

Generate iconset names required by `iconutil` and Android legacy sizes 48/72/96/144/192 px.

- [ ] **Step 6: Run smoke and commit**

Run:

```bash
rtk bash scripts/generate-app-icons.sh
rtk bash scripts/icon-assets-smoke.sh
rtk git diff --check
```

Expected: all pass.

Commit: `feat(brand): add deterministic B3 icon assets`

### Task 2: macOS Bundle and Menu-Bar Integration

**Files:**
- Modify: `scripts/build-macos-app-bundle.sh`
- Modify: `apps/macos/Sources/MHSaveSyncMac/main.swift`
- Modify: `scripts/icon-assets-smoke.sh`

- [ ] **Step 1: Extend the failing bundle smoke**

Add checks that the built bundle contains:

```bash
test -f 'artifacts/macos/MH Save Sync.app/Contents/Resources/MHSaveSync.icns'
test -f 'artifacts/macos/MH Save Sync.app/Contents/Resources/mh-save-sync-menubar-template.png'
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' \
  'artifacts/macos/MH Save Sync.app/Contents/Info.plist')" = 'MHSaveSync'
```

- [ ] **Step 2: Run the macOS smoke and verify RED**

Run: `rtk bash scripts/build-macos-app-bundle.sh && rtk bash scripts/icon-assets-smoke.sh`  
Expected: missing bundle resource or `CFBundleIconFile` assertion.

- [ ] **Step 3: Package resources and plist metadata**

In `scripts/build-macos-app-bundle.sh`:

```bash
cp apps/macos/Resources/AppIcon/MHSaveSync.icns "$resources/MHSaveSync.icns"
cp apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png \
  "$resources/mh-save-sync-menubar-template.png"
```

Add to generated `Info.plist`:

```xml
<key>CFBundleIconFile</key>
<string>MHSaveSync</string>
```

- [ ] **Step 4: Load a template image in the status item**

In `applicationDidFinishLaunching`, replace text-only initialization with:

```swift
if let url = Bundle.main.url(forResource: "mh-save-sync-menubar-template", withExtension: "png"),
   let image = NSImage(contentsOf: url) {
    image.isTemplate = true
    image.size = NSSize(width: 18, height: 18)
    item.button?.image = image
    item.button?.imagePosition = .imageLeading
}
item.button?.title = context.menuBarTitle
```

Keep the existing status title for accessibility and current-state visibility.

- [ ] **Step 5: Build, inspect and commit**

Run:

```bash
rtk bash scripts/build-macos-app-bundle.sh
rtk bash scripts/icon-assets-smoke.sh
rtk plutil -lint 'artifacts/macos/MH Save Sync.app/Contents/Info.plist'
rtk codesign -d --verbose=2 'artifacts/macos/MH Save Sync.app'
```

Expected: bundle build and all resource checks pass.

Commit: `feat(macos): ship app and menu bar icons`

### Task 3: Android Adaptive, Monochrome and Notification Integration

**Files:**
- Create: `apps/android/app/src/main/res/values/colors.xml`
- Create: `apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml`
- Create: `apps/android/app/src/main/res/drawable/ic_launcher_monochrome.xml`
- Create: `apps/android/app/src/main/res/drawable/ic_stat_save_sync.xml`
- Create: `apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml`
- Create: `apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher_round.xml`
- Modify: `apps/android/app/src/main/AndroidManifest.xml`
- Modify: `apps/android/app/src/main/java/org/mhtoolkit/savesync/ActiveSessionService.kt`
- Modify: `scripts/icon-assets-smoke.sh`

- [ ] **Step 1: Add failing Android resource assertions**

Require manifest icon/roundIcon attributes, adaptive XML foreground/background/monochrome items, and `ic_stat_save_sync` usage in notification construction.

- [ ] **Step 2: Run Android smoke and verify RED**

Run: `rtk bash scripts/icon-assets-smoke.sh`  
Expected: missing adaptive or notification resources.

- [ ] **Step 3: Add platform-native vectors**

Use a 108×108 viewport, keep the foreground inside the 66×66 safe zone, and express the shield/save/check as filled paths. `ic_launcher_monochrome.xml` and `ic_stat_save_sync.xml` must be single-color alpha-compatible vectors with no background rectangle.

- [ ] **Step 4: Add adaptive declarations and manifest references**

Adaptive icon XML:

```xml
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/icon_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />
</adaptive-icon>
```

Manifest application attributes:

```xml
android:icon="@mipmap/ic_launcher"
android:roundIcon="@mipmap/ic_launcher_round"
```

- [ ] **Step 5: Wire the notification icon**

Replace the generic framework icon in `ActiveSessionService` with:

```kotlin
.setSmallIcon(R.drawable.ic_stat_save_sync)
```

Apply the same resource to any other app-owned notification builder found by `rtk rg -n 'setSmallIcon' apps/android/app/src/main`.

- [ ] **Step 6: Build and commit**

Run:

```bash
rtk env JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  ./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug --no-daemon
rtk bash scripts/icon-assets-smoke.sh
rtk aapt dump badging apps/android/app/build/outputs/apk/debug/app-debug.apk | rtk rg 'icon=' 
```

Expected: tests/lint/build pass and badging resolves `@mipmap/ic_launcher`.

Commit: `feat(android): ship adaptive and notification icons`

### Task 4: Cross-Platform Visual and Artifact Verification

**Files:**
- Modify: `docs/runbooks/PHASE1_VALIDATION.md`
- Create: `artifacts/runtime/icon-system-evidence.json`

- [ ] **Step 1: Run full local gates**

```bash
rtk bash scripts/secret-scan.sh
rtk bash scripts/icon-assets-smoke.sh
rtk bash scripts/build-macos-app-bundle.sh
rtk env JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  ./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug --no-daemon
rtk git diff --check
```

- [ ] **Step 2: Inspect generated images**

Render or open the 1024 px app icon, 36 px menu template, Android legacy icon and notification vector. Verify transparent corners where required, no clipping, no text and recognizable shield/save/check geometry.

- [ ] **Step 3: Verify real macOS bundle**

Install with `rtk bash scripts/install-macos-app.sh`, then verify `/Applications/MH Save Sync.app` bundle resources and capture a Dock/Finder/menu-bar screenshot without exposing paths or secrets.

- [ ] **Step 4: Verify Android APK without touching Nemessix**

Install only package `org.mhtoolkit.savesync`, never `io.github.vincentadamnemessisx.nemessix`. Capture launcher/application-info and notification evidence; verify the installed Nemessix version/certificate remain unchanged using read-only package inspection.

- [ ] **Step 5: Record evidence and commit**

Write `artifacts/runtime/icon-system-evidence.json` with commit, artifact SHA-256 values, tested masks/modes, build commands and redacted evidence paths. Update `docs/runbooks/PHASE1_VALIDATION.md` with the icon-system verification result.

Commit: `test(brand): record cross-platform icon evidence`

### Task 5: Push and PR Verification

**Files:**
- No product files unless CI reveals a defect.

- [ ] **Step 1: Push the feature branch**

Run: `rtk git push -u origin feat/phase1-save-sync`

- [ ] **Step 2: Verify PR and CI**

Run:

```bash
rtk gh pr view 1 --json url,headRefName,baseRefName,state,statusCheckRollup
rtk gh run list --branch feat/phase1-save-sync --limit 10
```

Expected: PR remains open against `main`; local core integrity gates and icon resource gates pass. CI queue delay is not a blocker for continuing local verification.
