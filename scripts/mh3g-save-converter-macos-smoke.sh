#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_root="${1:-${MH3G_CONVERTER_MACOS_APP_ROOT:-$repo_root/artifacts/mh3g-save-converter-macos}}"
app="$app_root/MH3G Save Converter.app"
ui="$app/Contents/MacOS/MH3GSaveConverterMac"
cli="$app/Contents/MacOS/mh3g-save-convert"

[[ -x "$ui" ]] || { echo "missing UI binary: $ui" >&2; exit 1; }
[[ -x "$cli" ]] || { echo "missing bundled CLI: $cli" >&2; exit 1; }

fixture="$(mktemp -d "${TMPDIR:-/tmp}/mh3g-save-converter-smoke.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
source="$fixture/3ds/user2"
target="$fixture/cemu/user2"
mkdir -p "$(dirname "$source")" "$(dirname "$target")"

python3 - "$source" <<'PY'
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
payload = bytearray(0x8A00)
payload[:4] = b"\x2b\x00\x00\x00"
source.write_bytes(payload)
PY

source_before="$(shasum -a 256 "$source" | awk '{print $1}')"
"$cli" --help >/dev/null
"$cli" inspect "$source" | python3 -c 'import json, sys; assert json.load(sys.stdin)["status"] == "inspected"'
"$cli" convert "$source" --output "$target" --dry-run | python3 -c 'import json, sys; assert json.load(sys.stdin)["status"] == "dry-run"'
[[ ! -e "$target" ]]
"$cli" convert "$source" --output "$target" --write | python3 -c 'import json, sys; assert json.load(sys.stdin)["status"] == "written"'
manifest="$(dirname "$target")/.user2.mh3g-install.json"
[[ -f "$manifest" ]]
"$cli" rollback --manifest "$manifest" | python3 -c 'import json, sys; assert json.load(sys.stdin)["status"] == "rolled-back"'
[[ ! -e "$target" ]]
[[ "$(shasum -a 256 "$source" | awk '{print $1}')" == "$source_before" ]]

"$ui" --diagnostics | python3 -c '
import json, sys
value = json.load(sys.stdin)
assert value["ui_version"]
assert value["cli_version"].startswith("mh3g-save-convert ")
'

printf 'macOS app synthetic smoke passed: %s\n' "$app"
