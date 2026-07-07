# Emulator Save Adapter Matrix

- Status: Phase 1 evidence ledger, not final compatibility claim
- Last updated: 2026-07-07
- Host: macOS arm64; Android device evidence includes PKG110 Android 16 and
  AVD `Pixel_9_API_36_Daily` / `emulator-5554` (`sdk_gphone64_arm64`).
- Evidence rule: `Runtime Verified` requires a real build plus snapshot, mutate/damage, restore, and emulator-readable round trip. Package install, source-path proof, or fixture tests are not enough.
- Privacy rule: this file records title IDs, counts, sizes, hashes, package/bundle IDs, and source locations only. It does not record character names, save filenames, ROM paths, keys, or file contents.

## 1. Adapter support matrix

| Adapter | Platform | Bundle / package / process | User-root acquisition | Save path contract | Launch / exit / save-complete capability | Restore precondition | Support level now | Evidence fingerprint |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Nemessix 3DS | macOS | Bundle `io.github.vincentadamnemessisx.nemessix`, app `/Applications/Nemessix.app`, version `c836bc1`, executable md5 `7e933c163ed6a9b55fd284ef26eaff08` | Native per-user path discovered from Azahar/Nemessix source and local Application Support. | `~/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/<system>/<sd>/title/<high>/<low>/data/00000001/`; includes only save data and required metadata. | Process lifecycle observable through macOS process APIs; no authenticated `save-complete` IPC exists yet. FSEvents can mark dirty only. | Emulator stopped; same-volume staging backup before replace. | Path Verified; not yet Runtime Verified in this ledger because the save→mutate→restore→game-readable loop is still pending. | Local MH title roots observed: `0004000000048100` = 3 files / 53,764 bytes / tree fp `63ae25d28d41f210`; `000400000004b500` = 1 file / 512 bytes; `000400000011d700` = 1 file / 512 bytes. |
| Nemessix 3DS | Android | Package `io.github.vincentadamnemessisx.nemessix`, `versionName=f0767428c-vanilla`, `versionCode=33157070`, non-debuggable `run-as` denied. | SAF tree grant required; current observed shared-storage root `/storage/emulated/0/Games/Nemessix` is readable via ADB but the product must not rely on shell/root. | Same Citra/Azahar SDMC title layout under the SAF root. Shader, texture, cheat, cache, screenshots and config excluded by default. | Foreground service for active session; FileObserver/SAF reconciliation dirty-only; OneTimeWorkRequest after exit; PeriodicWorkRequest for 15-minute-class backstop. No save-complete IPC yet. | SAF journaled restore only when package/process stopped; never live overwrite. | Path Verified via ADB; runtime restore loop pending. | `0004000000048100` root observed under `Games/Nemessix`: 2 files / 47,616 bytes / aggregate sha256 `dd93905a1a8ee7875d9db8ee21a58e96f9c5519f5b2038f88cfe63129e4565ca`. |
| Azahar | Android | Package `org.azahar_emu.azahar`, `versionName=2126.0-alpha2-vanilla`, `versionCode=33012474`, non-debuggable `run-as` denied. | SAF tree grant required. No current Azahar save root was observed on shared storage in this pass. | Source-derived Citra/Azahar SDMC layout; exact user root must be selected/confirmed through SAF. | Same Android policy as Nemessix. No save-complete IPC. | SAF journaled restore with package stopped. | Experimental until a real Azahar data root and restore round trip exist. | Installed package verified; ADB search found no `/storage/emulated/0/(Games/)Azahar/.../data/00000001` save root on 2026-07-04. |
| Citra MMJ / classic-derived Android | Android | Package `org.citra.emu`, `versionName=20220729-mh-rpc.2`, `versionCode=45988`, non-debuggable `run-as` denied. | Current legacy shared root `/storage/emulated/0/citra-emu` is ADB-readable; production client must request SAF grant or fail closed. | `citra-emu/sdmc/Nintendo 3DS/<system>/<sd>/title/<high>/<low>/data/00000001/`. | FileObserver/SAF dirty-only; no reliable save-complete IPC. Process exit from package lifecycle/usage stats is advisory unless shell/helper managed. | SAF journaled restore with package stopped. | Path Verified; runtime restore loop pending. | `0004000000048100` root observed: 2 files / 47,616 bytes / aggregate sha256 `dd93905a1a8ee7875d9db8ee21a58e96f9c5519f5b2038f88cfe63129e4565ca`. |
| Generic Folder | macOS / Android | User-selected folder, not tied to emulator | Native path on macOS; SAF tree on Android; ADB shared storage used only for evidence capture. | Adapter-descriptor includes/excludes define relative roots; rejects symlink, absolute path, `..`, duplicates, case collision and size bombs. | No save-complete; watcher/observer dirty-only plus manual/periodic reconcile. | No managed emulator: restore requires explicit user confirmation or configured stopped-lock provider. | Generic Folder shared-storage verified; not an emulator Runtime Verified claim. | 2026-07-07 AVD `/sdcard/MHSaveSyncE2E`: macOS Generic Folder HEAD uploaded to public Alpha API, Android shared-storage divergent branch retained as conflict, cloud HEAD restored byte-for-byte back to `/sdcard`; restored sha256 `d92bf81eb5f71918292b1c5515792135574123c8c98c52da0a242492e3703268`, logical save `adb-generic-folder-1783427004776726000`. |
| Azahar | macOS | Expected bundle `org.azahar_emu.azahar` / upstream Azahar family | Native path; source uses platform-specific app data directories with optional configured user path. | Same SDMC save layout. | Process lifecycle + FSEvents. | Emulator stopped. | Experimental until a real installed build/data root exists. | No active `/Applications/Azahar.app` data root used in this pass. |
| Citra classic | macOS | Citra-family bundle/process | Native path, legacy app data directories | Same SDMC save layout | Process lifecycle + FSEvents | Emulator stopped | Experimental | No current installed build/data round trip evidence in this pass. |
| PPSSPP | macOS / Android | Descriptor planned for `org.ppsspp.ppsspp` and desktop app family | Native/SAF user root | `PSP/SAVEDATA/<game-slot>`-style contract to be confirmed from official PPSSPP source/docs before enabling restore | Dirty-only until save-complete source is proven | Emulator stopped | Descriptor contract only | Not Runtime Verified. |
| Dolphin | macOS / Android | Descriptor planned for Dolphin app/package family | Native/SAF user root | Wii/GC title save roots to be confirmed from official Dolphin source/docs | Dirty-only | Emulator stopped | Descriptor contract only | Not Runtime Verified. |
| PCSX2 / NetherSX2 | macOS / Android | Descriptor planned for PCSX2/NetherSX2 family | Native/SAF user root | Memory-card file contract; card image must be treated as one logical save unless validated per-title extraction exists | Dirty-only | Emulator stopped | Descriptor contract only | Not Runtime Verified. |
| Switch emulator family | macOS / Android | Descriptor planned only; no keys/firmware/ROM assumptions | Native/SAF user root | Per-title save container contract must be verified from legal emulator source and user-provided data | Dirty-only | Emulator stopped | Descriptor contract only | Not Runtime Verified. |

## 2. Source-derived 3DS path contract

Official/source evidence used for the 3DS emulator family:

- Azahar local source `src/common/file_util.cpp`: constructs `UserPath::SDMCDir = user_path + "sdmc/"` and separates config, cache, shader, dump, load, state and cheat roots from save roots. Decision: include save data only; exclude `shaders`, `load/textures`, `cheats`, `cache`, screenshots/dumps and config by default.
- Azahar local source `src/core/file_sys/archive_source_sd_savedata.cpp`: `GetSaveDataContainerPath(sdmc) = sdmc + "Nintendo 3DS/<SYSTEM_ID>/<SDCARD_ID>/title/"` and `GetSaveDataPath(program_id) = <high>/<low>/data/00000001/`. Decision: `GameKey` stores exact title ID and region/update identity; no implicit 3G/3U/4G/4U/XX/GU conversion.
- Azahar Android source `src/android/app/src/main/java/org/citra/citra_emu/adapters/GameAdapter.kt`: UI derives `saveDir`, DLC, updates, extra data, mods and texture directories separately. Decision: adapter explicitly maps save, extdata, DLC/update read-only references and excludes mods/textures unless a future profile opts in.
- Android official docs for Storage Access Framework: persistable tree URI grants are the supported model for durable user-selected document trees. Decision: Android adapters require SAF/IPC access and fail closed when another app sandbox or tree root is unavailable; root and Accessibility are not requirements.
- AndroidX WorkManager official docs: `PeriodicWorkRequest.MIN_PERIODIC_INTERVAL_MILLIS` is 15 minutes. Decision: periodic reconciliation cannot assume sub-15-minute scheduling; exit-triggered one-time work and visible foreground service handle active windows.
- Apple File System Events / CoreServices docs: FSEvents reports directory-tree changes but is not an application-level save-complete signal. Decision: FSEvents marks dirty only.

## 3. Adapter descriptor requirements

Each descriptor must serialize at least:

```text
emulator_id
platform
bundle_ids / package_ids / process_names
root_acquisition: native_path | SAF_tree | authenticated_IPC
supported_game_keys: title_id + title_family + region + update + slot mapping
include_globs / exclude_globs
capabilities: save_complete, launch_gate, exit_reconcile, running_state, saf_restore_journal
stability_validator: min_observations, max_wait, manifest schema, root fingerprint function
restore_preconditions: emulator_stopped, same_volume_atomic_replace or SAF_journal
support_level: RuntimeVerified | PathVerified | FixtureVerified | Experimental
evidence_fingerprint: build id + package/bundle id + path proof + redacted tree fingerprint + test run id
```

## 4. Current adoption decisions

- Adopt one 3DS-family path contract with emulator-specific root acquisition and evidence scoping. Nemessix, Azahar, Citra MMJ and Citra classic are separate descriptors even if their directory layout overlaps.
- Adopt explicit title IDs for Monster Hunter variants. `0004000000048100`, `000400000004B500`, and `000400000011D700` are separate `GameKey`s. No automatic region conversion, title migration or semantic merge is allowed in phase 1.
- Reject treating installed Android packages as accessible. Non-debuggable `run-as` denial proves the production app must use SAF or emulator IPC and fail closed otherwise.
- Reject uploading texture/shader/cache/cheat/config trees by default even when they live beside saves.
- Keep Azahar Android, Azahar macOS, Citra classic macOS, PPSSPP, Dolphin, PCSX2/NetherSX2 and Switch-family adapters Experimental until each has real legal data and restore evidence.
- Upgrade Generic Folder from pure fixture evidence to Android shared-storage
  evidence for generic user-selected folders only. This does not upgrade
  Nemessix, Azahar or Citra-family adapters because no emulator reopened and
  read the restored data.

## 5. Runtime verification checklist

A descriptor can be upgraded to `Runtime Verified` only when the evidence bundle contains:

1. package/bundle/process identity and app build hash;
2. root acquisition method that a normal user can grant without root;
3. pre-save tree fingerprint;
4. emulator save action or authenticated save-complete event;
5. stable snapshot ID created from staging, not watcher-direct upload;
6. controlled mutation or damage in the emulator-visible save root;
7. restore from the chosen snapshot while emulator stopped;
8. emulator-readable confirmation after relaunch;
9. conflict branch proof if another device committed on the same base;
10. redacted logs proving no recovery phrase, token, plaintext path content or save bytes were emitted.

## 6. MH Save Sync Android client evidence

This section validates the save-sync Android shell, not emulator runtime
compatibility.

2026-07-05 local evidence:

```text
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" ./gradlew assembleDebug lintDebug
Result: BUILD SUCCESSFUL
APK sha256: ccfa6f5b9d842cb2c363c4d2338a5a8777039ba8ad16f04d96c92c4ee860a307
```

The Android app contains:

- Compose status surface for SAF grant, active session and manual reconcile
  state;
- persisted SAF URI handling;
- `ForegroundService` scaffold for active emulator sessions;
- `OneTimeWorkRequest` and periodic WorkManager scheduling helpers with
  Wi-Fi/battery/charging constraints.

Earlier ADB smoke evidence showed the debug APK installed and launched on the
attached Android device, and app-private preferences recorded
`last_reconcile_reason=manual`. This evidence proves client-shell plumbing
only. A Nemessix/Azahar/Citra descriptor still requires the runtime checklist
above before it can be marked `Runtime Verified`.
