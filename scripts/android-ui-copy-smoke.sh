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

python3 - "$tmpdir" "$device_serial" "$package_name" <<'PY'
from pathlib import Path
import json
import re
import sys
import xml.sax.saxutils

tmpdir, device_serial, package_name = sys.argv[1:4]
texts = []
for path in sorted(Path(tmpdir).glob("*.xml")):
    xml_text = path.read_text(errors="replace")
    for value in re.findall(r'text="([^"]*)"', xml_text):
        if value:
            texts.append(xml.sax.saxutils.unescape(value))

joined = "\n".join(texts)
required = [
    "MH 云存档同步",
    "办公室 Mac 和回家 Android",
    "同步路线：MH3G / Android Nemessix",
    "不会静默覆盖",
    "当前状态和下一步",
    "选择 Android Nemessix 存档目录",
    "同步到哪里",
    "服务器地址",
    "MH3G 同步开关",
    "启动前检查",
]
missing = [item for item in required if item not in joined]
if missing:
    raise SystemExit("missing Android UI copy: " + ", ".join(missing) + "\n--- visible text ---\n" + joined)

print(json.dumps({
    "android_ui_copy_smoke": True,
    "device_serial": device_serial,
    "package": package_name,
    "required_copy_count": len(required),
    "visible_text_sha256": __import__("hashlib").sha256(joined.encode()).hexdigest(),
}, ensure_ascii=False, sort_keys=True))
PY
