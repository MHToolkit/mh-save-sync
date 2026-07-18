# MH Save Sync Apple Design Inventory

Date: 2026-07-18  
Scope: `apps/android` Compose dashboard and `apps/macos` AppKit menu-bar app.

## Real UI surfaces

| Surface | Existing entry points | User workflow | Current gap / contract |
| --- | --- | --- | --- |
| Android dashboard | `MainActivity.SaveSyncDashboard` | status, upload, restore, launch check, recent record | Status previously mixed phase/error prose and did not expose a stable semantic tone. The dashboard now maps success, neutral queue, warning/conflict and error into `SaveSyncUiStatePresentation`; every blocking state has an explicit next action. |
| Android conflict | `ConflictDialog`, `LocalReplaceCloudConfirmDialog`, `RestoreCloudConfirmDialog` | inspect branches, choose local/cloud, defer | No automatic choice and no LWW are preserved. Dialogs remain native Material 3 alerts. The status card announces unresolved conflict count and says it will not auto-cover. |
| Android offline/queue | `CardSection("存档状态")`, settings queue line | retain uploads while offline and resume by original endpoint | Queue status now says the number retained and that the original server address is used; no silent migration or overwrite. |
| Android error | `SaveSyncUiStatePresentation` plus existing `persistSyncStatus` | retry after network/key/directory failure | Error tone is actionable and explicitly says local/cloud data were not silently overwritten. |
| macOS menu bar | `MenuController` in `apps/macos/Sources/MHSaveSyncMac/main.swift` | sync, upload, restore, conflict, cloud status, setup/help | Uses native `NSStatusItem`, `NSMenu`, `NSAlert`, keyboard equivalents and disabled history item. Existing transaction recovery and failure alert remain fail-closed. |
| SwiftUI / Web | No SwiftUI or Web target exists in this repository | N/A | **Unverified / not applicable** for this repo. The shared organization baseline must be implemented by each owning product when those targets are added; this change does not invent a duplicate web surface. |

## Shared semantic contract

`SaveSyncDesignTokens.kt` is the Android implementation of the organization baseline for this product: system light/dark colors, 20dp screen rhythm, 14dp section rhythm, 8/10dp content rhythm, 180ms status response and 240ms content expansion. Tokens are semantic (`success`, `warning`, `error`) and are not tied to a specific feature.

`SaveSyncUiStatePresentation` is the state contract:

- `Success`: ready or session-protected; tell the user the next safe action.
- `Neutral`: queued/offline; state how many items are retained and that the queue resumes by its original endpoint.
- `Warning`: unresolved conflict or missing setup; block destructive actions and name the choice required.
- `Error`: failed operation; offer a retry path without claiming a write, restore or rollback happened.

Android maps the contract to Material 3 colors, `animateContentSize` (interruptible layout response), system dynamic font sizing, and an accessibility `LiveRegionMode.Polite` status announcement. macOS maps equivalent states to the native status-item label, menu enablement and `NSAlert` confirmation/error paths; system menu and alert behavior supplies Reduce Motion/Increase Contrast handling.

## Accessibility and motion checks

- No motion is used to communicate a state exclusively; text state and next action are always present.
- Compose status changes use a polite live region, preserving screen-reader order.
- Material controls keep platform hit targets, focus, keyboard and contrast behavior.
- System font scaling is retained; no fixed text size was introduced.
- Native macOS menus/alerts remain the accessibility boundary; disabled history is explicitly unavailable rather than clickable.

## Test and evidence entry points

- State contract red/green test: `apps/android/app/src/test/java/org/mhtoolkit/savesync/SaveSyncUiStateTest.kt`.
- Existing dashboard/conflict tests: `DashboardContentPolicyTest.kt` and related unit tests.
- macOS non-launch preview: `swift run MHSaveSyncMac --menu-preview` (does not open Nemessix or a game).
- Android build/unit test: `JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" ./gradlew :app:testDebugUnitTest`.
- UI screenshot/device evidence is tracked separately in `docs/design/evidence-2026-07-18.md`; no formal game process is started by these checks.
