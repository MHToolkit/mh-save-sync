#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

adb_bin="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
package_name="${MH_SAVE_SYNC_ANDROID_PACKAGE:-org.mhtoolkit.savesync}"
apk_path="${MH_SAVE_SYNC_APK:-$repo_root/apps/android/app/build/outputs/apk/debug/app-debug.apk}"
launch_timeout="${MH_SAVE_SYNC_APK_LAUNCH_TIMEOUT:-20}"

[[ -x "$adb_bin" ]] || blocked "adb not found at $adb_bin"
[[ -f "$apk_path" ]] || blocked "APK not found at $apk_path; run apps/android ./gradlew assembleDebug first or set MH_SAVE_SYNC_APK"

device_count="$("$adb_bin" devices | awk 'NR>1 && $2=="device" {count++} END {print count+0}')"
[[ "$device_count" -eq 1 ]] || blocked "expected exactly one online adb device, found $device_count"
device_serial="$("$adb_bin" devices | awk 'NR>1 && $2=="device" {print $1; exit}')"

boot="$("$adb_bin" -s "$device_serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r' || true)"
[[ "$boot" == "1" ]] || blocked "adb device $device_serial is not boot-completed"

apk_sha256="$(shasum -a 256 "$apk_path" | awk '{print $1}')"

"$adb_bin" -s "$device_serial" logcat -c >/dev/null 2>&1 || true
"$adb_bin" -s "$device_serial" install -r "$apk_path" >/tmp/mh-save-sync-apk-install.log
if ! grep -Eq 'Success|Performing Streamed Install' /tmp/mh-save-sync-apk-install.log; then
  cat /tmp/mh-save-sync-apk-install.log >&2
  exit 1
fi

"$adb_bin" -s "$device_serial" shell monkey -p "$package_name" -c android.intent.category.LAUNCHER 1 >/tmp/mh-save-sync-apk-monkey.log 2>&1

resumed=""
for _ in $(seq 1 "$launch_timeout"); do
  resumed="$("$adb_bin" -s "$device_serial" shell dumpsys activity activities 2>/dev/null | grep -E 'mResumedActivity|topResumedActivity' | head -n 3 || true)"
  if grep -q "$package_name" <<<"$resumed"; then
    break
  fi
  sleep 1
done

if ! grep -q "$package_name" <<<"$resumed"; then
  echo "Android app did not become resumed activity within ${launch_timeout}s" >&2
  echo "$resumed" >&2
  "$adb_bin" -s "$device_serial" shell dumpsys window | grep -E 'mCurrentFocus|mFocusedApp' >&2 || true
  exit 1
fi

crash_log="$("$adb_bin" -s "$device_serial" logcat -d -t 1200 2>/dev/null | grep -E "FATAL EXCEPTION|Process: ${package_name}" || true)"
if [[ -n "$crash_log" ]]; then
  echo "Android crash log detected after launch:" >&2
  echo "$crash_log" >&2
  exit 1
fi

python3 - "$device_serial" "$package_name" "$apk_path" "$apk_sha256" "$resumed" <<'PY'
import json
import sys

device_serial, package_name, apk_path, apk_sha256, resumed = sys.argv[1:6]
print(json.dumps({
    "android_apk_smoke": True,
    "device_serial": device_serial,
    "package": package_name,
    "apk": apk_path,
    "apk_sha256": apk_sha256,
    "resumed_activity": resumed.strip(),
}, ensure_ascii=False, sort_keys=True))
PY
