#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

avd_name="${MH_SAVE_SYNC_AVD_NAME:-Pixel_9_API_36_Daily}"
server_url="${MH_SAVE_SYNC_SERVER_URL:-}"
[[ -n "$server_url" ]] || blocked "set MH_SAVE_SYNC_SERVER_URL to a running mh-save-sync API"

sdk_root="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
emulator_bin="${EMULATOR:-$sdk_root/emulator/emulator}"
adb_bin="${ADB:-$sdk_root/platform-tools/adb}"
[[ -x "$emulator_bin" ]] || blocked "emulator not found at $emulator_bin"
[[ -x "$adb_bin" ]] || blocked "adb not found at $adb_bin"

log="${MH_SAVE_SYNC_AVD_LOG:-/tmp/mh-save-sync-avd-e2e.log}"
: > "$log"

"$adb_bin" kill-server >/dev/null 2>&1 || true
"$adb_bin" start-server >/dev/null
"$emulator_bin" \
  -avd "$avd_name" \
  -no-window \
  -no-audio \
  -no-boot-anim \
  -no-snapshot-load \
  -gpu swiftshader_indirect \
  -netdelay none \
  -netspeed full \
  >"$log" 2>&1 &
emulator_pid="$!"

cleanup() {
  status=$?
  "$adb_bin" emu kill >/dev/null 2>&1 || true
  kill "$emulator_pid" >/dev/null 2>&1 || true
  wait "$emulator_pid" >/dev/null 2>&1 || true
  exit "$status"
}
trap cleanup EXIT

boot=""
for _ in $(seq 1 "${MH_SAVE_SYNC_AVD_BOOT_ATTEMPTS:-150}"); do
  state="$($adb_bin get-state 2>/dev/null || true)"
  if [[ "$state" == "device" ]]; then
    boot="$($adb_bin shell getprop sys.boot_completed 2>/dev/null | tr -d "\r" || true)"
    if [[ "$boot" == "1" ]]; then
      break
    fi
  fi
  sleep 2
done

"$adb_bin" devices -l
if [[ "$boot" != "1" ]]; then
  echo "AVD did not reach sys.boot_completed=1" >&2
  tail -160 "$log" >&2 || true
  exit 77
fi

curl -fsS "${server_url%/}/ready" >/dev/null
MH_SAVE_SYNC_SERVER_URL="$server_url" ADB="$adb_bin" ./scripts/android-generic-folder-e2e.sh
