#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

required_copy_json='["MH 云存档","MH3G · Android Nemessix","使用帮助","存档状态","快速同步","上传手机存档","恢复云端存档","启动游戏","检查并打开 Nemessix","最近记录","设置"]'
forbidden_copy_json='["发现云端版本，请先选择上传或恢复。","MH 云存档同步","办公室 Mac 和回家 Android","同步路线：MH3G / Android Nemessix","当前状态和下一步","选择 Android Nemessix 存档目录","同步到哪里","MH3G 同步开关","启动前检查"]'

if [[ "${1:-}" == "--check-contract" ]]; then
  python3 - "$required_copy_json" "$forbidden_copy_json" \
    "$repo_root/apps/android/app/src/main/java/org/mhtoolkit/savesync/MainActivity.kt" \
    "$repo_root/apps/android/app/src/main/java/org/mhtoolkit/savesync/DashboardContentPolicy.kt" <<'PY'
import json
from pathlib import Path
import sys

required, forbidden = map(json.loads, sys.argv[1:3])
source = "\n".join(Path(path).read_text() for path in sys.argv[3:])
missing = [item for item in required if item not in source]
present_forbidden = [item for item in forbidden if item in source]
if missing:
    raise SystemExit("contract terms missing from compact Android UI source: " + ", ".join(missing))
if present_forbidden:
    raise SystemExit("legacy Android UI copy still present: " + ", ".join(present_forbidden))
print(json.dumps({
    "android_ui_copy_contract": True,
    "required_copy_count": len(required),
    "forbidden_copy_count": len(forbidden),
}, ensure_ascii=False, sort_keys=True))
PY
  exit 0
fi

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

adb_bin="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
package_name="${MH_SAVE_SYNC_ANDROID_PACKAGE:-org.mhtoolkit.savesync}"
apk_path="${MH_SAVE_SYNC_APK:-$repo_root/apps/android/app/build/outputs/apk/debug/app-debug.apk}"

[[ -x "$adb_bin" ]] || blocked "adb not found at $adb_bin"
[[ -f "$apk_path" ]] || blocked "APK not found at $apk_path; run apps/android ./gradlew assembleDebug first or set MH_SAVE_SYNC_APK"

device_count="$("$adb_bin" devices | awk 'NR>1 && $2=="device" {count++} END {print count+0}')"
[[ "$device_count" -eq 1 ]] || blocked "expected exactly one online adb device, found $device_count"
device_serial="$("$adb_bin" devices | awk 'NR>1 && $2=="device" {print $1; exit}')"

"$repo_root/scripts/android-apk-smoke.sh" >/tmp/mh-save-sync-apk-smoke-for-ui.json

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

dump_screen() {
  local name="$1"
  "$adb_bin" -s "$device_serial" shell uiautomator dump "/sdcard/${name}.xml" >/dev/null
  "$adb_bin" -s "$device_serial" pull "/sdcard/${name}.xml" "$tmpdir/${name}.xml" >/dev/null
}

dump_screen "mh-save-sync-ui-top"
"$adb_bin" -s "$device_serial" shell input swipe 500 1900 500 700 700 >/dev/null
sleep 1
dump_screen "mh-save-sync-ui-middle"

python3 - "$tmpdir" "$device_serial" "$package_name" "$required_copy_json" "$forbidden_copy_json" <<'PY'
from pathlib import Path
import json
import re
import sys
import xml.sax.saxutils

tmpdir, device_serial, package_name = sys.argv[1:4]
required = json.loads(sys.argv[4])
forbidden = json.loads(sys.argv[5])
texts = []
for path in sorted(Path(tmpdir).glob("*.xml")):
    xml_text = path.read_text(errors="replace")
    for value in re.findall(r'text="([^"]*)"', xml_text):
        if value:
            texts.append(xml.sax.saxutils.unescape(value))

joined = "\n".join(texts)
missing = [item for item in required if item not in joined]
if missing:
    raise SystemExit("missing Android UI copy: " + ", ".join(missing) + "\n--- visible text ---\n" + joined)
legacy = [item for item in forbidden if item in joined]
if legacy:
    raise SystemExit("forbidden legacy Android UI copy: " + ", ".join(legacy) + "\n--- visible text ---\n" + joined)

print(json.dumps({
    "android_ui_copy_smoke": True,
    "device_serial": device_serial,
    "package": package_name,
    "required_copy_count": len(required),
    "forbidden_copy_count": len(forbidden),
    "visible_text_sha256": __import__("hashlib").sha256(joined.encode()).hexdigest(),
}, ensure_ascii=False, sort_keys=True))
PY
