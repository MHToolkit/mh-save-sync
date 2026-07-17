# Android Home Device Validation Runbook

- Status: Phase 1 real-device handoff checklist
- Last updated: 2026-07-08
- Boundary: this runbook prepares evidence for Android phone validation. It
  does not by itself upgrade Nemessix, Azahar or Citra MMJ to Runtime Verified.

## 1. Pick an APK artifact

Generate or refresh the local Alpha APK first:

```bash
JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
MH_SAVE_SYNC_RUN_ADB_SMOKE=auto \
./scripts/android-package-alpha.sh
```

Then resolve the newest local handoff artifact without hard-coding a commit
suffix:

```bash
eval "$(./scripts/android-latest-alpha-apk.sh)"
printf 'APK=%s\nAPK_SHA256=%s\nEVIDENCE=%s\nEVIDENCE_SHA256=%s\n' \
  "$MH_SAVE_SYNC_APK" \
  "$MH_SAVE_SYNC_APK_SHA256" \
  "$MH_SAVE_SYNC_APK_EVIDENCE" \
  "$MH_SAVE_SYNC_APK_EVIDENCE_SHA256"
adb install -r "$MH_SAVE_SYNC_APK"
```

Use the APK evidence JSON, APK SHA256 and GitHub Actions run recorded next to
the APK as the artifact authority. Do not infer the APK build commit from this
runbook's Git commit: documentation or validation-script commits may happen
after the Android APK is built. If Android app code changes, rebuild the APK
and rerun `./scripts/android-latest-alpha-apk.sh` before real-device
validation.

## 2. Run device preflight

From the repository root:

```bash
eval "$(./scripts/android-latest-alpha-apk.sh)"
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
6. records the current repository HEAD separately from the APK SHA256;
7. writes `artifacts/runtime/android_home_device_preflight.json`.

Privacy boundary: it records package/activity/server facts only. It does not
enumerate save directories, filenames, save bytes, character names, ROM paths,
keys or recovery phrases.

## 3. Build a redacted runtime evidence bundle

After installing the APK and completing the in-app setup steps, collect a
metadata-only bundle:

```bash
eval "$(./scripts/android-latest-alpha-apk.sh)"
MH_SAVE_SYNC_SERVER_URL="http://8.130.112.207:39082" \
ADB="$HOME/Library/Android/sdk/platform-tools/adb" \
./scripts/android-runtime-evidence-bundle.sh
```

If multiple Android devices are attached, set `ANDROID_SERIAL=<serial>`.

For a real emulator-specific acceptance pass, add redacted operator metadata:

```bash
eval "$(./scripts/android-latest-alpha-apk.sh)"
MH_SAVE_SYNC_RUNTIME_TARGET_PACKAGE="io.github.vincentadamnemessisx.nemessix" \
MH_SAVE_SYNC_RUNTIME_TARGET_EMULATOR="android-nemessix" \
MH_SAVE_SYNC_LOGICAL_SAVE_ID="<copy from app/CLI status; no plaintext paths>" \
MH_SAVE_SYNC_SNAPSHOT_ID="<cloud/local snapshot id>" \
MH_SAVE_SYNC_CONFLICT_COUNT="<0 or conflict count shown by app>" \
MH_SAVE_SYNC_SAF_GRANT_CONFIRMED="true" \
MH_SAVE_SYNC_STOPPED_RESTORE_CONFIRMED="true" \
MH_SAVE_SYNC_READBACK_CONFIRMED="true" \
MH_SAVE_SYNC_CONFLICT_CONFIRMED="true" \
MH_SAVE_SYNC_REDACTED_LOGS_REVIEWED="true" \
MH_SAVE_SYNC_RUNTIME_NOTE="真实保存、退出后上传、停止状态恢复、重启后游戏可读；不含角色名/路径/密钥" \
MH_SAVE_SYNC_SERVER_URL="http://8.130.112.207:39082" \
ADB="$HOME/Library/Android/sdk/platform-tools/adb" \
./scripts/android-runtime-evidence-bundle.sh
```

The bundle writes:

- `android_home_device_preflight.json`;
- `runtime_evidence_audit.json`;
- package fact JSON for MH Save Sync and target emulator packages;
- `ui_visibility_summary.json` with UI text hash and required-copy booleans
  only;
- `runtime_claim.json` as a redacted checklist template;
- `<timestamp>.tar.gz` plus SHA256.

Privacy boundary: the bundle stores metadata, hashes, booleans and redacted
operator notes only. It does **not** enumerate save directories, copy save
files, record character names, print recovery phrases or include plaintext save
bytes.

## 4. Interpret results

If `runtime_targets_available=false`, the device can only support:

- APK install/launch evidence;
- Chinese UI visibility evidence;
- Generic Folder / shared-storage synthetic validation.

Do not mark Android Nemessix, Azahar or Citra MMJ Runtime Verified.

If `runtime_targets_available=true`, proceed to the real runtime checklist and
then rerun the bundle script:

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

The bundle summary can set `support_upgrade_ready=true` only when all required
RuntimeVerified checklist booleans are satisfied. The confirmation variables
above must be set only after the matching real action has been performed and
reviewed. A green APK install, server reachability, target package presence, or
free-form note is not enough by itself.

## 5. Cloud endpoint caveat

If `server_ready=false` and SSH to the server still works, check whether the
problem is Aliyun security group / firewall exposure. SSH access alone cannot
open Alibaba Cloud console security-group ports; if a cloud-console rule is
missing, the user must change it in the Aliyun console.

## 6. Evidence required before support-level upgrade

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
