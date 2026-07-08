#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/home" "$tmp/Applications"

if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="/" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted unsafe install dir" >&2
  exit 1
fi
if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" MH_SAVE_SYNC_APP_NAME="bad/name.app" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted app name with slash" >&2
  exit 1
fi
if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" MH_SAVE_SYNC_APP_NAME="bad" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted non-.app name" >&2
  exit 1
fi

output="$(HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" ./scripts/install-macos-app.sh)"
app="$tmp/Applications/MH Save Sync.app"
exe="$app/Contents/MacOS/MHSaveSyncMac"
test -x "$exe"
grep -q '"macos_app_installed": true' <<<"$output"

HOME="$tmp/home" "$exe" --set-server-url http://127.0.0.1:39082 >/dev/null
status="$(HOME="$tmp/home" "$exe" --status)"
grep -q "同步到服务器：http://127.0.0.1:39082" <<<"$status"
grep -q "当前同步对象：MH3G / macOS Nemessix" <<<"$status"

help="$(HOME="$tmp/home" "$exe" --help)"
grep -q "双击" <<<"$help"
grep -q -- "--set-server-url" <<<"$help"

python3 - <<PY
import json
print(json.dumps({
    "macos_install_e2e": True,
    "install_dir": "$tmp/Applications",
    "installed_app": "$app",
    "server_url_persisted": "http://127.0.0.1:39082",
}, ensure_ascii=False, sort_keys=True))
PY
