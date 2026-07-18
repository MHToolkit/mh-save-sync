#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "icon assets smoke: $*" >&2
  exit 1
}

assert_file() {
  test -f "$1" || fail "missing $1"
}

assert_png() {
  local path="$1"
  local expected_size="$2"
  local metadata
  metadata="$(sips -g pixelWidth -g pixelHeight -g hasAlpha "$path" 2>/dev/null)"
  grep -q "pixelWidth: ${expected_size}" <<<"$metadata" || fail "$path has wrong width"
  grep -q "pixelHeight: ${expected_size}" <<<"$metadata" || fail "$path has wrong height"
  grep -q 'hasAlpha: yes' <<<"$metadata" || fail "$path must retain transparency"
}

assert_file design/icon/mh-save-sync-icon.svg
grep -q 'stop-color="#4936B7"' design/icon/mh-save-sync-icon.svg || fail "SVG is missing B3 dark purple"
grep -q 'stop-color="#9B72F2"' design/icon/mh-save-sync-icon.svg || fail "SVG is missing B3 light purple"
assert_file apps/macos/Resources/AppIcon/MHSaveSync.icns
assert_file apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png
assert_png apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png 36

densities=(mdpi hdpi xhdpi xxhdpi xxxhdpi)
sizes=(48 72 96 144 192)
for index in "${!densities[@]}"; do
  density="${densities[$index]}"
  size="${sizes[$index]}"
  assert_file "apps/android/app/src/main/res/mipmap-${density}/ic_launcher.png"
  assert_file "apps/android/app/src/main/res/mipmap-${density}/ic_launcher_round.png"
  assert_png "apps/android/app/src/main/res/mipmap-${density}/ic_launcher.png" "$size"
  assert_png "apps/android/app/src/main/res/mipmap-${density}/ic_launcher_round.png" "$size"
done

swift scripts/verify-icon-pixels.swift

iconset_check="$(mktemp -d)"
trap 'rm -rf "$iconset_check"' EXIT
iconutil -c iconset apps/macos/Resources/AppIcon/MHSaveSync.icns -o "$iconset_check/MHSaveSync.iconset"
for representation in \
  icon_16x16.png icon_16x16@2x.png \
  icon_32x32.png icon_32x32@2x.png \
  icon_128x128.png icon_128x128@2x.png \
  icon_256x256.png icon_256x256@2x.png \
  icon_512x512.png icon_512x512@2x.png; do
  assert_file "$iconset_check/MHSaveSync.iconset/$representation"
done

scripts/generate-app-icons.sh --check | grep -q 'icon assets deterministic: ok' \
  || fail "icon generator output differs from checked-in assets"

android_res='apps/android/app/src/main/res'
android_manifest='apps/android/app/src/main/AndroidManifest.xml'
assert_file "${android_res}/drawable/ic_launcher_foreground.xml"
assert_file "${android_res}/drawable/ic_launcher_background.xml"
assert_file "${android_res}/drawable/ic_launcher_monochrome.xml"
assert_file "${android_res}/drawable/ic_stat_save_sync.xml"
assert_file "${android_res}/mipmap-anydpi-v26/ic_launcher.xml"
assert_file "${android_res}/mipmap-anydpi-v26/ic_launcher_round.xml"
assert_file "${android_res}/mipmap-anydpi-v33/ic_launcher.xml"
assert_file "${android_res}/mipmap-anydpi-v33/ic_launcher_round.xml"
assert_file "${android_res}/values/colors.xml"
grep -q '@color/icon_background' "${android_res}/drawable/ic_launcher_background.xml" \
  || fail "adaptive background must include B3 dark purple"
grep -q '@color/icon_background_light' "${android_res}/drawable/ic_launcher_background.xml" \
  || fail "adaptive background must include B3 light purple"

for alpha_icon in ic_launcher_monochrome ic_stat_save_sync; do
  alpha_path="${android_res}/drawable/${alpha_icon}.xml"
  grep -q 'android:fillType="evenOdd"' "$alpha_path" \
    || fail "$alpha_path must preserve save/check detail as alpha cut-outs"
  if grep -Eq '#4936B7|#9B72F2|@color/' "$alpha_path"; then
    fail "$alpha_path must remain a single-color alpha glyph"
  fi
done

python3 - <<'PY'
import re
import xml.etree.ElementTree as ET

path = "apps/android/app/src/main/res/drawable/ic_launcher_monochrome.xml"
root = ET.parse(path).getroot()
android = "{http://schemas.android.com/apk/res/android}"
for element in root.findall("path"):
    values = [
        float(value)
        for value in re.findall(r"-?\d+(?:\.\d+)?", element.attrib[android + "pathData"])
    ]
    if len(values) % 2:
        raise SystemExit(f"{path}: pathData must contain x/y coordinate pairs")
    for x, y in zip(values[0::2], values[1::2]):
        if not (21 <= x <= 87 and 21 <= y <= 87):
            raise SystemExit(
                f"{path}: coordinate ({x}, {y}) exceeds the 66x66 adaptive safe zone"
            )
PY

grep -q 'android:icon="@mipmap/ic_launcher"' "$android_manifest" \
  || fail "Android manifest must reference the launcher icon"
grep -q 'android:roundIcon="@mipmap/ic_launcher_round"' "$android_manifest" \
  || fail "Android manifest must reference the round launcher icon"
for adaptive_icon in ic_launcher ic_launcher_round; do
  v26_path="${android_res}/mipmap-anydpi-v26/${adaptive_icon}.xml"
  v33_path="${android_res}/mipmap-anydpi-v33/${adaptive_icon}.xml"
  for adaptive_path in "$v26_path" "$v33_path"; do
    grep -q 'android:drawable="@drawable/ic_launcher_background"' "$adaptive_path" \
      || fail "$adaptive_path must reference the B3 gradient background"
    grep -q 'android:drawable="@drawable/ic_launcher_foreground"' "$adaptive_path" \
      || fail "$adaptive_path must reference the color foreground"
  done
  if grep -q '<monochrome' "$v26_path"; then
    fail "$v26_path must not use the API 33 monochrome element"
  fi
  grep -q 'android:drawable="@drawable/ic_launcher_monochrome"' "$v33_path" \
    || fail "$v33_path must reference the monochrome foreground"
done

grep -R -q 'setSmallIcon(R.drawable.ic_stat_save_sync)' \
  apps/android/app/src/main/java \
  || fail "Android notifications must use the dedicated save-sync small icon"

android_apk='apps/android/app/build/outputs/apk/debug/app-debug.apk'
assert_file "$android_apk"
apk_entries="$(unzip -Z1 "$android_apk")"
for api in 26 33; do
  for adaptive_icon in ic_launcher ic_launcher_round; do
    grep -q "res/mipmap-anydpi-v${api}/${adaptive_icon}.xml" <<<"$apk_entries" \
      || fail "APK is missing the v${api} ${adaptive_icon} resource"
  done
done

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
aapt_candidates=("$sdk_root"/build-tools/*/aapt)
aapt_bin="${aapt_candidates[${#aapt_candidates[@]}-1]}"
test -x "$aapt_bin" || fail "Android aapt is unavailable under $sdk_root/build-tools"
packaged_resources="$("$aapt_bin" dump resources "$android_apk")"
grep -q 'config anydpi-v33:' <<<"$packaged_resources" \
  || fail "APK resource table is missing the API 33 adaptive icon override"
grep -q 'drawable/ic_launcher_monochrome' <<<"$packaged_resources" \
  || fail "APK resource table is missing the monochrome drawable"
grep -q 'drawable/ic_stat_save_sync' <<<"$packaged_resources" \
  || fail "APK resource table is missing the notification glyph"

file apps/macos/Resources/AppIcon/MHSaveSync.icns | grep -q 'Mac OS X icon' \
  || fail "MHSaveSync.icns is not a valid macOS icon container"

macos_bundle='artifacts/macos/MH Save Sync.app'
assert_file "${macos_bundle}/Contents/Resources/MHSaveSync.icns"
assert_file "${macos_bundle}/Contents/Resources/mh-save-sync-menubar-template.png"
bundle_icon="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' \
  "${macos_bundle}/Contents/Info.plist" 2>/dev/null || true)"
test "$bundle_icon" = 'MHSaveSync' \
  || fail "macOS bundle CFBundleIconFile must be MHSaveSync"

echo "icon assets smoke: ok"
