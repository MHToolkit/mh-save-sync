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

file apps/macos/Resources/AppIcon/MHSaveSync.icns | grep -q 'Mac OS X icon' \
  || fail "MHSaveSync.icns is not a valid macOS icon container"

echo "icon assets smoke: ok"
