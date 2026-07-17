#!/usr/bin/env bash
set -euo pipefail

# Collect a redacted Android real-device evidence bundle for Phase 1 runtime
# verification handoff.  This script is deliberately metadata-only: it records
# package/activity/server/SAF-grant counts and user-visible checklist facts, but
# never enumerates emulator save trees, pulls save files, prints recovery
# phrases, or records plaintext save bytes.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

adb="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
if [[ -n "${MH_SAVE_SYNC_APK:-}" ]]; then
  apk="$MH_SAVE_SYNC_APK"
else
  latest_resolver="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/scripts/android-latest-alpha-apk.sh"
  if apk="$(MH_SAVE_SYNC_APK_FORMAT=path "$latest_resolver" 2>/dev/null)"; then
    :
  else
    apk="apps/android/app/build/outputs/apk/debug/app-debug.apk"
  fi
  [[ -n "$apk" ]] || apk="apps/android/app/build/outputs/apk/debug/app-debug.apk"
fi
server_url="${MH_SAVE_SYNC_SERVER_URL:-}"
target_package="${MH_SAVE_SYNC_RUNTIME_TARGET_PACKAGE:-}"
target_emulator="${MH_SAVE_SYNC_RUNTIME_TARGET_EMULATOR:-}"
logical_save_id="${MH_SAVE_SYNC_LOGICAL_SAVE_ID:-}"
snapshot_id="${MH_SAVE_SYNC_SNAPSHOT_ID:-}"
conflict_count="${MH_SAVE_SYNC_CONFLICT_COUNT:-}"
manual_note="${MH_SAVE_SYNC_RUNTIME_NOTE:-}"
saf_grant_confirmed="${MH_SAVE_SYNC_SAF_GRANT_CONFIRMED:-false}"
stopped_restore_confirmed="${MH_SAVE_SYNC_STOPPED_RESTORE_CONFIRMED:-false}"
readback_confirmed="${MH_SAVE_SYNC_READBACK_CONFIRMED:-false}"
conflict_confirmed="${MH_SAVE_SYNC_CONFLICT_CONFIRMED:-false}"
redacted_logs_reviewed="${MH_SAVE_SYNC_REDACTED_LOGS_REVIEWED:-false}"
out_root="${MH_SAVE_SYNC_RUNTIME_BUNDLE_DIR:-artifacts/runtime/android-real-device}"

package="org.mhtoolkit.savesync"
target_packages=(
  "io.github.vincentadamnemessisx.nemessix"
  "org.azahar_emu.azahar"
  "org.citra.emu"
)

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

[[ -x "$adb" ]] || blocked "adb not found at $adb"
[[ -f "$apk" ]] || blocked "APK not found at $apk; run scripts/android-package-alpha.sh or set MH_SAVE_SYNC_APK"

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

boot="$("$adb" -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r' || true)"
[[ "$boot" == "1" ]] || blocked "adb device $serial is not boot-completed"

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
bundle_dir="$out_root/$timestamp"
mkdir -p "$bundle_dir"

preflight_json="$bundle_dir/android_home_device_preflight.json"
runtime_audit_json="$bundle_dir/runtime_evidence_audit.json"
ui_summary_json="$bundle_dir/ui_visibility_summary.json"
device_summary_json="$bundle_dir/device_package_summary.json"
runtime_claim_json="$bundle_dir/runtime_claim.json"
summary_json="$bundle_dir/evidence_bundle_summary.json"

MH_SAVE_SYNC_HOME_PREFLIGHT_OUT="$preflight_json" \
MH_SAVE_SYNC_HOME_RUNTIME_AUDIT_OUT="$runtime_audit_json" \
ADB="$adb" \
MH_SAVE_SYNC_APK="$apk" \
MH_SAVE_SYNC_SERVER_URL="$server_url" \
ANDROID_SERIAL="$serial" \
  ./scripts/android-home-device-preflight.sh >/dev/null

tmp="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

packages_file="$tmp/packages.txt"
"$adb" -s "$serial" shell pm list packages | sed 's/^package://' | tr -d '\r' | sort > "$packages_file"

collect_pkg_fact() {
  local pkg="$1"
  local out="$2"
  if ! grep -Fxq "$pkg" "$packages_file"; then
    printf '{"package":"%s","installed":false}\n' "$pkg" > "$out"
    return
  fi
  local dump="$tmp/${pkg//[^A-Za-z0-9_.-]/_}.dumpsys.txt"
  "$adb" -s "$serial" shell dumpsys package "$pkg" > "$dump" 2>/dev/null || true
  python3 - "$pkg" "$dump" > "$out" <<'PY'
import json
import re
import sys
from pathlib import Path

pkg, dump_path = sys.argv[1:3]
text = Path(dump_path).read_text(encoding="utf-8", errors="replace")
version_name = ""
version_code = ""
first_install = ""
last_update = ""
for line in text.splitlines():
    stripped = line.strip()
    if stripped.startswith("versionName="):
        version_name = stripped.split("=", 1)[1]
    elif stripped.startswith("versionCode="):
        version_code = stripped.split("=", 1)[1].split()[0]
    elif stripped.startswith("firstInstallTime="):
        first_install = stripped.split("=", 1)[1]
    elif stripped.startswith("lastUpdateTime="):
        last_update = stripped.split("=", 1)[1]

persisted_uri_permission_count = sum(
    1 for line in text.splitlines()
    if "UriPermission" in line or "persisted" in line and "uri=" in line
)
requested_permissions = sorted(set(re.findall(r"android\.permission\.[A-Z0-9_]+", text)))
print(json.dumps({
    "package": pkg,
    "installed": True,
    "version_name": version_name,
    "version_code": version_code,
    "first_install_time": first_install,
    "last_update_time": last_update,
    "persisted_uri_permission_count": persisted_uri_permission_count,
    "requested_permission_count": len(requested_permissions),
    "requested_permissions": requested_permissions,
}, ensure_ascii=False, sort_keys=True))
PY
}

pkg_facts_dir="$bundle_dir/package_facts"
mkdir -p "$pkg_facts_dir"
collect_pkg_fact "$package" "$pkg_facts_dir/mh_save_sync.json"
for pkg in "${target_packages[@]}"; do
  collect_pkg_fact "$pkg" "$pkg_facts_dir/${pkg//[^A-Za-z0-9_.-]/_}.json"
done

"$adb" -s "$serial" shell uiautomator dump /sdcard/mh-save-sync-runtime-ui.xml >/dev/null 2>&1 || true
if "$adb" -s "$serial" pull /sdcard/mh-save-sync-runtime-ui.xml "$tmp/ui.xml" >/dev/null 2>&1; then
  python3 - "$tmp/ui.xml" > "$ui_summary_json" <<'PY'
from pathlib import Path
import hashlib
import json
import re
import sys
import xml.sax.saxutils

xml_text = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
texts = [xml.sax.saxutils.unescape(v) for v in re.findall(r'text="([^"]*)"', xml_text) if v]
joined = "\n".join(texts)
required = [
    "MH 云存档同步",
    "同步到哪里",
    "启动前检查",
    "不会静默覆盖",
    "服务器地址",
]
print(json.dumps({
    "ui_dump_available": True,
    "visible_text_count": len(texts),
    "visible_text_sha256": hashlib.sha256(joined.encode()).hexdigest(),
    "required_copy_present": {item: item in joined for item in required},
    "privacy_boundary": "stores only text hash and required-copy booleans, not the visible text body",
}, ensure_ascii=False, sort_keys=True))
PY
else
  printf '{"ui_dump_available":false,"privacy_boundary":"no UI text captured"}\n' > "$ui_summary_json"
fi

server_ready=false
server_status_sha256=""
if [[ -n "$server_url" ]]; then
  server_url="${server_url%/}"
  if curl -fsS --max-time 10 "$server_url/ready" > "$tmp/ready.json" 2>/dev/null; then
    server_ready=true
    server_status_sha256="$(shasum -a 256 "$tmp/ready.json" | awk '{print $1}')"
  fi
fi

apk_sha256="$(shasum -a 256 "$apk" | awk '{print $1}')"
repo_head="$(git rev-parse HEAD 2>/dev/null || true)"
matched_targets=()
for pkg in "${target_packages[@]}"; do
  if grep -Fxq "$pkg" "$packages_file"; then
    matched_targets+=("$pkg")
  fi
done

python3 - "$device_summary_json" "$serial" "$apk" "$apk_sha256" "$repo_head" "$server_url" "$server_ready" "$server_status_sha256" "${matched_targets[*]-}" <<'PY'
import json
import sys

(
    out,
    serial,
    apk,
    apk_sha256,
    repo_head,
    server_url,
    server_ready,
    server_status_sha256,
    matched_raw,
) = sys.argv[1:10]
matched = [p for p in matched_raw.split() if p]
from pathlib import Path
Path(out).write_text(json.dumps({
    "device_serial": serial,
    "apk": apk,
    "apk_sha256": apk_sha256,
    "repo_head": repo_head,
    "server_url_configured": bool(server_url),
    "server_ready": server_ready == "true",
    "server_status_sha256": server_status_sha256,
    "matched_runtime_target_packages": matched,
    "runtime_targets_available": bool(matched),
    "privacy_boundary": "package/server identifiers and hashes only; no save tree enumeration",
}, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

target_package_installed=false
if [[ -n "$target_package" ]] && grep -Fxq "$target_package" "$packages_file"; then
  target_package_installed=true
fi

python3 - "$runtime_claim_json" "$target_package" "$target_package_installed" "$target_emulator" "$logical_save_id" "$snapshot_id" "$conflict_count" "$manual_note" "$saf_grant_confirmed" "$stopped_restore_confirmed" "$readback_confirmed" "$conflict_confirmed" "$redacted_logs_reviewed" <<'PY'
import json
import sys
from pathlib import Path

(
    out,
    target_package,
    target_package_installed,
    target_emulator,
    logical_save_id,
    snapshot_id,
    conflict_count,
    manual_note,
    saf_grant_confirmed,
    stopped_restore_confirmed,
    readback_confirmed,
    conflict_confirmed,
    redacted_logs_reviewed,
) = sys.argv[1:14]

def as_bool(value: str) -> bool:
    return value.strip().lower() in {"1", "true", "yes", "y"}

required = {
    "target_package_installed": as_bool(target_package_installed),
    "saf_grant_recorded_by_app": as_bool(saf_grant_confirmed),
    "stable_snapshot_id_recorded": bool(snapshot_id),
    "stopped_restore_confirmed": as_bool(stopped_restore_confirmed),
    "emulator_relaunch_readback_confirmed": as_bool(readback_confirmed),
    "conflict_branch_confirmed_if_divergent": as_bool(conflict_confirmed) or conflict_count in {"", "0"},
    "redacted_logs_reviewed": as_bool(redacted_logs_reviewed),
}
claim = {
    "runtime_claim_template": True,
    "target_package": target_package,
    "target_emulator": target_emulator,
    "logical_save_id": logical_save_id,
    "snapshot_id": snapshot_id,
    "conflict_count": conflict_count,
    "manual_note_redacted": manual_note,
    "support_upgrade_candidate": all(required.values()),
    "required_for_runtime_verified": required,
    "privacy_boundary": "operator-filled IDs and redacted notes only; do not paste recovery secrets, paths, character names, or save bytes",
}
Path(out).write_text(json.dumps(claim, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

tar_path="${bundle_dir}.tar.gz"
tar -czf "$tar_path" -C "$out_root" "$timestamp"
bundle_sha256="$(shasum -a 256 "$tar_path" | awk '{print $1}')"

python3 - "$summary_json" "$bundle_dir" "$tar_path" "$bundle_sha256" "$preflight_json" "$runtime_audit_json" "$device_summary_json" "$ui_summary_json" "$runtime_claim_json" <<'PY'
import json
import sys
from pathlib import Path

(
    summary,
    bundle_dir,
    tar_path,
    bundle_sha256,
    preflight_json,
    runtime_audit_json,
    device_summary_json,
    ui_summary_json,
    runtime_claim_json,
) = sys.argv[1:10]

device = json.loads(Path(device_summary_json).read_text(encoding="utf-8"))
claim = json.loads(Path(runtime_claim_json).read_text(encoding="utf-8"))
ready_for_upgrade = (
    device.get("server_ready") is True
    and device.get("runtime_targets_available") is True
    and all(claim.get("required_for_runtime_verified", {}).values())
)
summary_obj = {
    "android_runtime_evidence_bundle": True,
    "bundle_dir": bundle_dir,
    "bundle_tar_gz": tar_path,
    "bundle_sha256": bundle_sha256,
    "server_ready": device.get("server_ready"),
    "runtime_targets_available": device.get("runtime_targets_available"),
    "support_upgrade_ready": ready_for_upgrade,
    "artifacts": {
        "preflight": preflight_json,
        "runtime_audit": runtime_audit_json,
        "device_summary": device_summary_json,
        "ui_summary": ui_summary_json,
        "runtime_claim": runtime_claim_json,
    },
    "next_actions_zh": [
        "如果 runtime_targets_available=false，先在手机安装/配置目标 Android 模拟器，不能升级 RuntimeVerified。",
        "如果 support_upgrade_ready=false，继续完成 SAF 授权、真实保存、退出后上传、停止状态恢复、重启模拟器读档、冲突分支与脱敏日志复核。",
        "不要把恢复密钥、明文路径、角色名或存档字节粘进 runtime_claim.json。",
    ],
    "privacy_boundary": "metadata bundle only; no save files, no save tree listing, no recovery phrase, no token",
}
Path(summary).write_text(json.dumps(summary_obj, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(summary_obj, ensure_ascii=False, sort_keys=True))
PY

printf '%s\n' "BUNDLE_DIR: $bundle_dir"
printf '%s\n' "BUNDLE_TAR_GZ: $tar_path"
printf '%s\n' "BUNDLE_SHA256: $bundle_sha256"
