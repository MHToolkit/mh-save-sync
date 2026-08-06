#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:-$repo_root/artifacts}"
app_root="$output_dir/mh3g-save-converter-macos"
app="$app_root/MH3G Save Converter.app"
archive="$output_dir/MH3G-Save-Converter-macOS-arm64.zip"
checksum="$archive.sha256"
verify_dir="$output_dir/.mh3g-save-converter-package-verify"

mkdir -p "$output_dir"
MH3G_CONVERTER_MACOS_APP_ROOT="$app_root" "$repo_root/scripts/build-mh3g-save-converter-macos-app.sh"
"$repo_root/scripts/mh3g-save-converter-macos-smoke.sh" "$app_root"

if [[ "${MH3G_CONVERTER_PACKAGE_VALIDATE_ONLY:-0}" == "1" ]]; then
  printf 'validation-only app: %s\n' "$app"
  exit 0
fi

rm -f "$archive" "$checksum"
ditto -c -k --keepParent "$app" "$archive"
(
  cd "$output_dir"
  shasum -a 256 "$(basename "$archive")" > "$(basename "$checksum")"
)

rm -rf "$verify_dir"
mkdir -p "$verify_dir"
ditto -x -k "$archive" "$verify_dir"
"$repo_root/scripts/mh3g-save-converter-macos-smoke.sh" "$verify_dir"
rm -rf "$verify_dir"

printf 'package: %s\nchecksum: %s\n' "$archive" "$checksum"
