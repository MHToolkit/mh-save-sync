#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_root="${MH3G_CONVERTER_MACOS_APP_ROOT:-$repo_root/artifacts/mh3g-save-converter-macos}"
app_dir="$app_root/MH3G Save Converter.app"
contents="$app_dir/Contents"
macos="$contents/MacOS"
resources="$contents/Resources"
version="${MH3G_CONVERTER_UI_VERSION:-0.0.7}"
build_number="${MH3G_CONVERTER_UI_BUILD:-1}"

[[ "$(uname -s)" == "Darwin" ]] || { echo "macOS app packaging must run on macOS" >&2; exit 1; }
[[ "$(uname -m)" == "arm64" ]] || { echo "macOS app packaging currently requires an arm64 Apple Silicon host" >&2; exit 1; }
[[ "$build_number" =~ ^[1-9][0-9]*$ ]] || { echo "MH3G_CONVERTER_UI_BUILD must be a positive integer" >&2; exit 2; }

cd "$repo_root"
cargo build --locked --release -p mh3g-save-convert --bin mh3g-save-convert
swift build -c release --package-path apps/mh3g-save-converter-macos

rm -rf "$app_dir"
mkdir -p "$macos" "$resources/Artwork"

install -m 0755 target/release/mh3g-save-convert "$macos/mh3g-save-convert"
install -m 0755 apps/mh3g-save-converter-macos/.build/release/MH3GSaveConverterMac "$macos/MH3GSaveConverterMac"
install -m 0644 apps/mh3g-save-converter-macos/Resources/AppIcon/MH3GSaveConverter.icns "$resources/MH3GSaveConverter.icns"
install -m 0644 apps/mh3g-save-converter-macos/Resources/Artwork/*.png "$resources/Artwork/"
install -m 0644 README.md "$resources/README.md"
install -m 0644 README.zh-CN.md "$resources/README.zh-CN.md"

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>MH3G Save Converter</string>
  <key>CFBundleExecutable</key><string>MH3GSaveConverterMac</string>
  <key>CFBundleIdentifier</key><string>org.mhtoolkit.mh3g-save-converter</string>
  <key>CFBundleIconFile</key><string>MH3GSaveConverter</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>MH3G Save Converter</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${build_number}</string>
  <key>LSMinimumSystemVersion</key><string>15.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

plutil -lint "$contents/Info.plist" >/dev/null
"$macos/mh3g-save-convert" --help >/dev/null
"$macos/MH3GSaveConverterMac" --diagnostics | python3 -c '
import json, sys
value = json.load(sys.stdin)
assert value["bundled_cli"].endswith("Contents/MacOS/mh3g-save-convert")
assert value["cli_version"].startswith("mh3g-save-convert ")
'
"$macos/MH3GSaveConverterMac" --window-smoke | python3 -c '
import json, sys
assert json.load(sys.stdin)["visible_window_count"] >= 1
'

codesign --force --sign - --timestamp=none "$app_dir" >/dev/null
codesign --verify --deep --strict "$app_dir"

printf '{"macos_app_bundle":true,"path":"%s"}\n' "$app_dir"
