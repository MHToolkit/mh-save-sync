#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

swift build --package-path apps/macos
cargo build -q -p save-cli --bin mh-save

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  if [[ "${CARGO_TARGET_DIR}" = /* ]]; then
    cargo_target_dir="${CARGO_TARGET_DIR}"
  else
    cargo_target_dir="${repo_root}/${CARGO_TARGET_DIR}"
  fi
else
  cargo_target_dir="${repo_root}/target"
fi
cli_bin="${cargo_target_dir}/debug/mh-save"
test -x "$cli_bin"

app_dir="${repo_root}/artifacts/macos/MH Save Sync.app"
contents="${app_dir}/Contents"
macos="${contents}/MacOS"
resources="${contents}/Resources"
rm -rf "$app_dir"
mkdir -p "$macos" "$resources"

cp "${repo_root}/apps/macos/.build/debug/MHSaveSyncMac" "${macos}/MHSaveSyncMac"
cp "$cli_bin" "${macos}/mh-save"
chmod 755 "${macos}/MHSaveSyncMac" "${macos}/mh-save"

cat > "${contents}/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>zh_CN</string>
  <key>CFBundleDisplayName</key>
  <string>MH 云存档</string>
  <key>CFBundleExecutable</key>
  <string>MHSaveSyncMac</string>
  <key>CFBundleIdentifier</key>
  <string>org.mhtoolkit.mh-save-sync.alpha</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>MH Save Sync</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0-alpha</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>15.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

plutil -lint "${contents}/Info.plist" >/dev/null
test -x "${macos}/MHSaveSyncMac"
test -x "${macos}/mh-save"
"${macos}/MHSaveSyncMac" --prelaunch-check >/dev/null
"${macos}/mh-save" --help >/dev/null

printf '{"macos_app_bundle":true,"bundled_cli":true,"path":"%s"}\n' "$app_dir"
