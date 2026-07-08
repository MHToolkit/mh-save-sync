#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

install_dir="${MH_SAVE_SYNC_INSTALL_DIR:-/Applications}"
app_name="${MH_SAVE_SYNC_APP_NAME:-MH Save Sync.app}"
source_app="$repo_root/artifacts/macos/MH Save Sync.app"

if [[ -z "$install_dir" || "$install_dir" == "/" ]]; then
  echo "Refusing unsafe MH_SAVE_SYNC_INSTALL_DIR: '$install_dir'" >&2
  exit 2
fi
if [[ -z "$app_name" || "$app_name" == */* || "$app_name" != *.app ]]; then
  echo "Refusing unsafe MH_SAVE_SYNC_APP_NAME: '$app_name' (must be a single .app bundle name)" >&2
  exit 2
fi

dest_app="$install_dir/$app_name"
if [[ "$dest_app" == "/" || "$dest_app" == "$install_dir" || "$dest_app" != *.app ]]; then
  echo "Refusing unsafe destination app path: '$dest_app'" >&2
  exit 2
fi
if [[ -e "$dest_app" && ! -d "$dest_app" ]]; then
  echo "Refusing to replace non-directory destination: '$dest_app'" >&2
  exit 2
fi

./scripts/build-macos-app-bundle.sh >/dev/null

mkdir -p "$install_dir"
rm -rf "$dest_app"
if command -v ditto >/dev/null 2>&1; then
  ditto "$source_app" "$dest_app"
else
  cp -R "$source_app" "$dest_app"
fi
chmod -R u+rwX,go+rX "$dest_app"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$dest_app" >/dev/null 2>&1 || true
fi

plist="$dest_app/Contents/Info.plist"
exe="$dest_app/Contents/MacOS/MHSaveSyncMac"
plutil -lint "$plist" >/dev/null
test -x "$exe"
status_output="$("$exe" --status)"
grep -q "MH 云存档同步" <<<"$status_output"
grep -q "同步到服务器" <<<"$status_output"

python3 - <<PY
import json
print(json.dumps({
    "macos_app_installed": True,
    "path": "$dest_app",
    "display_name": "MH 云存档",
    "launch": "open '$dest_app'",
    "menu_bar_note": "打开后屏幕右上角显示 MH 云存档，Dock 不常驻",
    "config_command": "$exe --set-server-url <server-url>",
}, ensure_ascii=False, sort_keys=True))
PY
