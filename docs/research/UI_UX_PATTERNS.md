# UI/UX Pattern Research — MH Save Sync Alpha

- Access date: 2026-07-08
- Scope: Android Chinese-first sync workbench and macOS menu-bar utility for office Mac ↔ home Android save sync.
- Rule: UI must explain target, state, next action, conflict choices and unavailable-cloud behavior without exposing internal sync jargon.

## Sources reviewed

| Source | URL | Relevant guidance | Adopt / reject |
| --- | --- | --- | --- |
| Android Developers — Material 3 in Compose | https://developer.android.google.cn/develop/ui/compose/designsystems/material3?hl=en | Material 3 provides component suites such as buttons, app bars and navigation components for different use cases and screen sizes. | Adopt: keep Android in Compose Material 3; use standard components instead of custom visual chrome. |
| Android Developers — Card | https://developer.android.com/develop/ui/compose/components/card | Cards are Material containers for a single coherent piece of content. | Adopt: keep each sync section as one clear card: server, folder permission, launch gate, actions, recent status. |
| Android Developers — App bars | https://developer.android.com/develop/ui/compose/components/app-bars | App bars provide access to key tasks and information; top bars host titles and core actions. | Adopt for next UI pass: add a top app bar/title area with one primary status summary instead of a long raw page title only. |
| Android Developers — Dialog | https://developer.android.com/develop/ui/compose/components/dialog | Dialogs interrupt users for confirmation, input or option selection; AlertDialog is appropriate for simple two-button decisions. | Adopt: destructive/overwrite restore must use explicit confirmation; conflict explanation can remain informational until a real conflict list exists. |
| Apple HIG — Menus | https://developer.apple.com/design/human-interface-guidelines/menus | Menu labels should clearly and succinctly describe actions; important or frequently used items should appear first; related items should be grouped. | Adopt: macOS menu-bar app should put guide/server/start-check actions at the top and use verb labels. |
| Apple HIG — Feedback | https://developer.apple.com/design/human-interface-guidelines/feedback | Feedback should help users know what is happening, what they can do next, and avoid mistakes; passive status belongs near the item it describes, while data-loss warnings may interrupt. | Adopt: recent status and next-action lines stay in-screen; restore/overwrite requires interruptive confirmation. |
| Apple HIG — Alerts | https://developer.apple.com/design/human-interface-guidelines/alerts | Alerts interrupt tasks, so they should be used sparingly and only for important actionable information. | Adopt with constraint: first-run macOS menu-bar guide is acceptable while server is unconfigured; after configuration, avoid repeated startup alerts. |
| Android Developers — Background work | https://developer.android.com/develop/background-work | Foreground services should be used for critical visible work with clear notification; background APIs should respect system limits. | Adopt: active MH3G session uses visible notification; periodic work remains a conservative fallback, not a high-frequency poller. |
| Dropbox Help — sync icons | https://help.dropbox.com/sync/sync-icons | Dropbox exposes file sync state through visible icons such as synced, syncing, available offline and errors. | Adopt the principle: always show visible sync state and error state; reject copying file-level icon metaphors because this app syncs logical saves, not arbitrary files. |
| Google Drive Help — Drive for desktop | https://support.google.com/drive/answer/10838124 | Drive for desktop teaches users where desktop content lives, how sync works, and what offline availability means. | Adopt the principle: explain where data is going and which device/folder is involved; reject a pure background-only sync model. |

## Link verification

On 2026-07-08, a local verification script fetched every URL above with HTTP 200 and recorded page titles for Android Developers, Apple Developer Documentation, Dropbox Help and Google Drive Help.

## Competitive / ecosystem takeaways

- **One visible source of truth**: Popular sync/backup products keep a visible current state and next action. For MH Save Sync this maps to `最近同步`, `当前后台任务`, `下一步动作`, and explicit failure reason.
- **Primary action per state**: Avoid a wall of equal-weight buttons. The next design pass should elevate exactly one safe primary action for each state: configure server, authorize folder, run launch check, download-only, or restore after stop.
- **Progressive disclosure**: Normal users need route and result, not storage/protocol internals. Engineering terms belong in research/ADR docs, not first-run UI or player guide.
- **Dangerous restore is a gate**: Any action that can replace local saves must be stopped-emulator only, preceded by local backup, and confirmed in a dedicated dialog.
- **Menu-bar apps need discoverability**: Because a menu-bar utility may not appear in the Dock, first launch must explain where it lives and where setup actions are.

## Adopted UI direction for phase1 alpha

1. Keep Chinese-only first-run copy for phase1.
2. Keep Material 3 cards, but convert the Android screen into a state-first workbench:
   - top status summary;
   - setup card;
   - launch gate card;
   - action card;
   - recent status card.
3. Show server target and sync target before every action that talks to the cloud.
4. Replace internal jargon with user language: server, local cache, backup, restore, keep local, cloud version, conflict pending.
5. Keep macOS as a menu-bar Alpha app, but make it discoverable via startup guide and a persistent `新手引导` menu item.
6. Do not claim visual polish is complete until a screenshot-based review passes on both macOS and Android.

## Rejected for phase1

- Custom game-themed chrome before the data-safety flow is stable; it risks looking novelty-heavy and harder to trust.
- Silent background sync UI with only a spinner; the user must always know whether data went to the server, local cache, or nowhere.
- Repeated startup alerts after setup; they become noise and violate the “alerts sparingly” guidance.
- Semantic merge UI for binary saves; the honest conflict UI is choose-local, choose-cloud, or keep both pending.
