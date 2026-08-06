# Apple Design current-ref implementation

Date: 2026-08-06  
Source baseline: `cb5e55bf6921` (`v0.0.11`)  
Scope: MH Save Sync on Android/macOS and the MH3G Save Converter on macOS. The Windows converter keeps WinUI conventions and shares the same safety/state vocabulary; this change does not copy macOS glass onto Windows.

## Product inventory

| Surface | Primary job | Safety-critical states | Current implementation |
| --- | --- | --- | --- |
| Android Save Sync | authorize a directory, inspect cloud state, upload or restore with explicit confirmation | setup required, conflict, queued, protected play session, error, ready | semantic `SaveSyncUiStatePresentation`, Material 3 theme tokens, live-region next action, bounded content response |
| macOS Save Sync | expose sync direction and state from a menu-bar utility | setup missing, Nemessix active, conflict, queued, success/failure | explicit status-item VoiceOver label and next-action help |
| macOS Save Converter | inspect, Dry Run, fingerprint-authorized transaction, manifest/rollback | needs input, needs inspection, ready for Dry Run, ambiguous repair revision, optional data blocked, authorized, running, success/failure | native split view plus an always-visible semantic status card and toolbar status; no state is encoded by color alone |
| Windows Save Converter | same guarded converter contract using WinUI | same transaction states | existing WinUI cards, InfoBars, automation names, and explicit rollback flow remain the platform baseline |

## Design system contract

- **State before decoration.** A closed semantic state model decides title, next action, blocking state and accessibility identity before a surface chooses a color or symbol.
- **Material with restraint.** The converter status card uses native material only when transparency is allowed; Reduce Transparency produces a solid platform control background.
- **Motion as feedback.** Status changes use a critically damped spring (`response 0.34`, no bounce). Reduce Motion removes the spring while retaining text/icon feedback. Android uses a bounded 240 ms content response and respects the platform animator scale.
- **Accessibility.** Dynamic system typography, explicit VoiceOver/TalkBack wording, polite live-region updates, high-contrast borders, and a text badge when Differentiate Without Color is enabled.
- **Fail closed.** Ambiguous repair versions, missing optional paths, stale hashes, in-flight operations and failures remain visibly blocked. The UI never turns a completed inspection into a write claim.

## Runtime evidence

- macOS converter source-built app: `/tmp/mh3g-save-converter-apple-design-app/MH3G Save Converter.app`
- Accessibility tree exposed `mh3g.converter.status.needsInput` with title, safety detail and `Write blocked` text.
- Screenshot: `docs/design/evidence/2026-08-06-mh3g-save-converter-macos.png`  
  SHA-256: `003eb189c41fb9da4d5fdab57babcf45ae98130e8fa8f16e5c8e6229c9052b08`
- The screenshot used no real save, ROM, account or formal application data.

## Verification matrix

| Gate | Result |
| --- | --- |
| Converter Swift build/tests | Passed: 43 tests |
| Android `testDebugUnitTest` / `lintDebug` / `assembleDebug` | Passed locally with JDK 17; debug APK SHA-256 `8c02676add7058a3607e722e7875cc4d3ee549447ff18a66ae2dc941f33c473d` |
| macOS Save Sync Swift tests | Passed: 17 Swift Testing cases |
| Rust converter tests | Passed: 205 tests across 8 suites |
| macOS packaged-app synthetic smoke | Passed from the source-built `/tmp` app; no real save used |
| Windows WinUI source contract | Passed; no current-head Hosted Windows publish was run for this UI-only slice |
| Real save conversion/write/rollback | Not run in this UI slice; transaction behavior remains covered by synthetic/unit fixtures |
| Android device install | Not run in this UI slice |

The UI work does not change the converter file format, synchronization protocol, restore semantics, or transaction contract, so no new data-format ADR is required.
