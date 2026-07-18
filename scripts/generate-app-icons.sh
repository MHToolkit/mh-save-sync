#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_icon_dir="$repo_root/apps/macos/Resources/AppIcon"

rm -rf "$app_icon_dir/MHSaveSync.iconset"
swift "$repo_root/scripts/generate-app-icons.swift" --out-root "$repo_root"
iconutil -c icns "$app_icon_dir/MHSaveSync.iconset" \
  -o "$app_icon_dir/MHSaveSync.icns"
rm -rf "$app_icon_dir/MHSaveSync.iconset"

echo "generated B3 app icons"
