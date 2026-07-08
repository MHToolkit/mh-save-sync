# Android Home Device Validation Runbook

- Status: Phase 1 real-device handoff checklist
- Last updated: 2026-07-08
- Boundary: this runbook prepares evidence for Android phone validation. It
  does not by itself upgrade Nemessix, Azahar or Citra MMJ to Runtime Verified.

## 1. Install the latest CI-green APK

Current local delivery artifact:

```bash
adb install -r /Users/vincentadamnemessis/Games/Backups/MHSaveSync/apk/mh-save-sync-72e1d4e-debug.apk
```

Evidence file:

```text
/Users/vincentadamnemessis/Games/Backups/MHSaveSync/apk/mh-save-sync-72e1d4e-debug.evidence.json
```

The artifact is tied to PR head
`72e1d4eae78a0d77f52ddd32abd781d61d4bb555` and CI run
`https://github.com/MHToolkit/mh-save-sync/actions/runs/28944268168`.

## 2. Run device preflight

From the repository root:

```bash
MH_SAVE_SYNC_APK="/Users/vincentadamnemessis/Games/Backups/MHSaveSync/apk/mh-save-sync-72e1d4e-debug.apk" \
MH_SAVE_SYNC_SERVER_URL="http://8.130.112.207:39082" \
ADB="$HOME/Library/Android/sdk/platform-tools/adb" \
./scripts/android-home-device-preflight.sh
```

If multiple Android devices are attached, set `ANDROID_SERIAL=<serial>`.

The script:

1. installs the APK with `adb install -r`;
2. launches `org.mhtoolkit.savesync`;
3. verifies the app becomes the resumed activity;
4. checks server `/ready` when `MH_SAVE_SYNC_SERVER_URL` is set;
5. checks whether Android Nemessix, Azahar or Citra MMJ packages are installed;
6. writes `artifacts/runtime/android_home_device_preflight.json`.

Privacy boundary: it records package/activity/server facts only. It does not
enumerate save directories, filenames, save bytes, character names, ROM paths,
keys or recovery phrases.

## 3. Interpret results

If `runtime_targets_available=false`, the device can only support:

- APK install/launch evidence;
- Chinese UI visibility evidence;
- Generic Folder / shared-storage synthetic validation.

Do not mark Android Nemessix, Azahar or Citra MMJ Runtime Verified.

If `runtime_targets_available=true`, proceed to the real runtime checklist:

1. open MH Save Sync;
2. configure the same server URL as the Mac;
3. import/select the recovery secret file through the app flow;
4. authorize the emulator save root with SAF;
5. run launch pre-check before starting MH3G;
6. make a real in-emulator save;
7. exit the emulator fully;
8. run sync/upload from MH Save Sync;
9. restore on the opposite device only while the emulator is stopped;
10. relaunch the emulator and prove the game reads the restored save.

Conflict acceptance requires a second branch:

1. both devices start from the same cloud HEAD;
2. disconnect one side or stop the server;
3. modify/save on both sides independently;
4. reconnect/upload;
5. verify two conflict branches are shown;
6. choose local, cloud or keep both explicitly.

## 4. Cloud endpoint caveat

If `server_ready=false` and SSH to the server still works, check whether the
problem is Aliyun security group / firewall exposure. SSH access alone cannot
open Alibaba Cloud console security-group ports; if a cloud-console rule is
missing, the user must change it in the Aliyun console.

## 5. Evidence required before support-level upgrade

The adapter can be upgraded to Runtime Verified only when the evidence bundle
contains:

- package identity and app build;
- SAF grant/root acquisition method;
- stable snapshot ID;
- controlled save mutation or damage;
- stopped restore;
- emulator-readable relaunch proof;
- conflict proof when two devices diverge;
- redacted logs showing no recovery phrase, token, plaintext path content or
  save bytes were emitted.
