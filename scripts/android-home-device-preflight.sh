#!/usr/bin/env bash
set -euo pipefail

# Prepare and audit a real Android phone/AVD before attempting emulator-specific
# RuntimeVerified save-sync evidence.  This script intentionally collects only
# app/package/server reachability facts; it does not enumerate save files or
# read emulator save contents.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

adb="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
apk="${MH_SAVE_SYNC_APK:-apps/android/app/build/outputs/apk/debug/app-debug.apk}"
server_url="${MH_SAVE_SYNC_SERVER_URL:-}"
out="${MH_SAVE_SYNC_HOME_PREFLIGHT_OUT:-artifacts/runtime/android_home_device_preflight.json}"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

[[ -x "$adb" ]] || blocked "adb not found at $adb"
[[ -f "$apk" ]] || blocked "APK not found at $apk; run Android assembleDebug or set MH_SAVE_SYNC_APK"

devices=()
while IFS= read -r device; do
  [[ -n "$device" ]] && devices+=("$device")
done < <("$adb" devices | awk 'NR>1 && $2=="device" {print $1}')
if [[ -n "${ANDROID_SERIAL:-}" ]]; then
  serial="$ANDROID_SERIAL"
else
  [[ "${#devices[@]}" -eq 1 ]] || blocked "expected exactly one online adb device or set ANDROID_SERIAL, found ${#devices[@]}"
  serial="${devices[0]}"
fi

mkdir -p "$(dirname "$out")"
tmp="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

package="org.mhtoolkit.savesync"
activity="org.mhtoolkit.savesync/.MainActivity"
target_packages=(
  "io.github.vincentadamnemessisx.nemessix"
  "org.azahar_emu.azahar"
  "org.citra.emu"
)

"$adb" -s "$serial" install -r "$apk" >/dev/null
"$adb" -s "$serial" shell monkey -p "$package" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1 || true
sleep 2
dumpsys_activity="$("$adb" -s "$serial" shell dumpsys activity activities 2>/dev/null || true)"
resumed="$(printf '%s\n' "$dumpsys_activity" | grep -E 'topResumedActivity|mResumedActivity' | grep "$package" | head -n 1 | tr -d '\r' || true)"
if [[ -z "$resumed" ]]; then
  blocked "MH Save Sync did not become resumed activity on $serial"
fi

packages_file="$tmp/packages.txt"
"$adb" -s "$serial" shell pm list packages | sed 's/^package://' | tr -d '\r' | sort > "$packages_file"

matched_targets=()
missing_targets=()
for pkg in "${target_packages[@]}"; do
  if grep -Fxq "$pkg" "$packages_file"; then
    matched_targets+=("$pkg")
  else
    missing_targets+=("$pkg")
  fi
done

server_ready=false
server_status=""
if [[ -n "$server_url" ]]; then
  server_url="${server_url%/}"
  if curl -fsS --max-time 10 "$server_url/ready" > "$tmp/ready.json" 2> "$tmp/ready.err"; then
    server_ready=true
    server_status="$(cat "$tmp/ready.json")"
  else
    server_status="$(cat "$tmp/ready.err")"
  fi
fi

audit_out="${MH_SAVE_SYNC_HOME_RUNTIME_AUDIT_OUT:-${out%.json}.runtime_audit.json}"
mkdir -p "$(dirname "$audit_out")"
ADB="$adb" python3 scripts/runtime-evidence-audit.py --output "$audit_out" >/dev/null

apk_sha256="$(shasum -a 256 "$apk" | awk '{print $1}')"
repo_head="$(git rev-parse HEAD 2>/dev/null || true)"
runtime_targets_available=false
if [[ "${#matched_targets[@]}" -gt 0 ]]; then
  runtime_targets_available=true
fi

python3 - "$out" "$serial" "$apk" "$apk_sha256" "$repo_head" "$resumed" "$server_url" "$server_ready" "$server_status" "$audit_out" "${matched_targets[*]-}" "${missing_targets[*]-}" <<'PY'
import json
import sys
from pathlib import Path

(
    out,
    serial,
    apk,
    apk_sha256,
    repo_head,
    resumed,
    server_url,
    server_ready,
    server_status,
    audit_path,
    matched_raw,
    missing_raw,
) = sys.argv[1:13]

matched = [p for p in matched_raw.split() if p]
missing = [p for p in missing_raw.split() if p]
runtime_targets_available = bool(matched)
next_actions = []
if not server_url:
    next_actions.append("设置 MH_SAVE_SYNC_SERVER_URL 后重跑，确认手机能访问同一台云存档服务器。")
elif server_ready != "true":
    next_actions.append("服务器 /ready 不可用；先确认网络、端口或阿里云安全组。")
if not runtime_targets_available:
    next_actions.append("当前设备未安装 Android Nemessix/Azahar/Citra MMJ；只能做 APK/UI/Generic Folder 验证，不能升级 emulator RuntimeVerified。")
else:
    next_actions.append("已发现目标模拟器包；下一步在 App 内授权对应 SAF 存档根目录，再执行真实保存→上传→恢复→游戏可读验收。")

report = {
    "android_home_device_preflight": True,
    "device_serial": serial,
    "apk": apk,
    "apk_sha256": apk_sha256,
    "repo_head": repo_head,
    "package": "org.mhtoolkit.savesync",
    "resumed_activity": resumed,
    "server_url_configured": bool(server_url),
    "server_ready": server_ready == "true",
    "server_status": server_status,
    "matched_runtime_target_packages": matched,
    "missing_runtime_target_packages": missing,
    "runtime_targets_available": runtime_targets_available,
    "runtime_audit_artifact": audit_path,
    "privacy_boundary": "package/activity/server facts only; no save tree enumeration",
    "next_actions_zh": next_actions,
}
Path(out).write_text(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(report, ensure_ascii=False, sort_keys=True))
PY

if [[ "$runtime_targets_available" != "true" ]]; then
  echo "NOTE: no Android emulator runtime target package found; RuntimeVerified evidence remains unavailable on this device." >&2
fi
