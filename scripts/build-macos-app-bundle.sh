#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

macos_version_name="${MH_SAVE_SYNC_MACOS_VERSION_NAME:-0.1.0-alpha.3}"
macos_version_code="${MH_SAVE_SYNC_MACOS_VERSION_CODE:-4}"
[[ -n "$macos_version_name" ]] || { echo "MH_SAVE_SYNC_MACOS_VERSION_NAME must not be blank" >&2; exit 2; }
[[ "$macos_version_code" =~ ^[1-9][0-9]*$ ]] || { echo "MH_SAVE_SYNC_MACOS_VERSION_CODE must be a positive integer" >&2; exit 2; }

swift build -c release --package-path apps/macos
cargo build --release -q -p save-cli --bin mh-save

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  if [[ "${CARGO_TARGET_DIR}" = /* ]]; then
    cargo_target_dir="${CARGO_TARGET_DIR}"
  else
    cargo_target_dir="${repo_root}/${CARGO_TARGET_DIR}"
  fi
else
  cargo_target_dir="${repo_root}/target"
fi
cli_bin="${cargo_target_dir}/release/mh-save"
test -x "$cli_bin"

app_dir="${repo_root}/artifacts/macos/MH Save Sync.app"
contents="${app_dir}/Contents"
macos="${contents}/MacOS"
resources="${contents}/Resources"
rm -rf "$app_dir"
mkdir -p "$macos" "$resources"

cp "${repo_root}/apps/macos/.build/release/MHSaveSyncMac" "${macos}/MHSaveSyncMac"
cp "$cli_bin" "${macos}/mh-save"
cp "${repo_root}/apps/macos/Resources/AppIcon/MHSaveSync.icns" \
  "${resources}/MHSaveSync.icns"
cp "${repo_root}/apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png" \
  "${resources}/mh-save-sync-menubar-template.png"
chmod 755 "${macos}/MHSaveSyncMac" "${macos}/mh-save"

cat > "${contents}/Info.plist" <<PLIST
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
  <key>CFBundleIconFile</key>
  <string>MHSaveSync</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>MH Save Sync</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
<string>${macos_version_name}</string>
  <key>CFBundleVersion</key>
  <string>${macos_version_code}</string>
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

codesign --force --sign - --timestamp=none "$app_dir" >/dev/null
codesign --verify --deep --strict "$app_dir"

printf '{"macos_app_bundle":true,"bundled_cli":true,"path":"%s"}\n' "$app_dir"
