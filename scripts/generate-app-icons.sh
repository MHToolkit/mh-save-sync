#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

generate_into() {
  local output_root="$1"
  local app_icon_dir="$output_root/apps/macos/Resources/AppIcon"
  rm -rf "$app_icon_dir/MHSaveSync.iconset"
  swift "$repo_root/scripts/generate-app-icons.swift" --out-root "$output_root"
  iconutil -c icns "$app_icon_dir/MHSaveSync.iconset" \
    -o "$app_icon_dir/MHSaveSync.icns"
  rm -rf "$app_icon_dir/MHSaveSync.iconset"
}

if [[ "${1:-}" == "--check" ]]; then
  check_root="$(mktemp -d)"
  trap 'rm -rf "$check_root"' EXIT
  generate_into "$check_root"

  generated_files=(
    apps/macos/Resources/AppIcon/MHSaveSync.icns
    apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png
  )
  for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
    generated_files+=(
      "apps/android/app/src/main/res/mipmap-${density}/ic_launcher.png"
      "apps/android/app/src/main/res/mipmap-${density}/ic_launcher_round.png"
    )
  done
  for path in "${generated_files[@]}"; do
    cmp "$repo_root/$path" "$check_root/$path"
  done
  echo "icon assets deterministic: ok"
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: generate-app-icons.sh [--check]" >&2
  exit 2
fi

generate_into "$repo_root"

echo "generated B3 app icons"
