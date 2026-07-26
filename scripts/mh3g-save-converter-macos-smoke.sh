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
write_stdout="$fixture/write.stdout"
write_stderr="$fixture/write.stderr"
if "$cli" convert "$source" --output "$target" --write >"$write_stdout" 2>"$write_stderr"; then
  python3 - "$write_stdout" <<'PY'
import json
import pathlib
import sys

assert json.loads(pathlib.Path(sys.argv[1]).read_text())["status"] == "written"
PY
  manifest="$(dirname "$target")/.user2.mh3g-install.json"
  [[ -f "$manifest" ]]
  "$cli" rollback --manifest "$manifest" | python3 -c 'import json, sys; assert json.load(sys.stdin)["status"] == "rolled-back"'
  [[ ! -e "$target" ]]
else
  write_status=$?
  if grep -Fq 'unsafe install refused: emulator process is running:' "$write_stderr"; then
    [[ ! -e "$target" ]] || {
      echo "emulator safety guard failed: synthetic target was created" >&2
      exit 1
    }
    printf 'macOS app synthetic write/rollback skipped: emulator safety guard verified\n'
  else
    cat "$write_stderr" >&2
    [[ ! -s "$write_stdout" ]] || cat "$write_stdout" >&2
    exit "$write_status"
  fi
fi
[[ "$(shasum -a 256 "$source" | awk '{print $1}')" == "$source_before" ]]

"$ui" --diagnostics | python3 -c '
import json, sys
value = json.load(sys.stdin)
assert value["ui_version"]
assert value["cli_version"].startswith("mh3g-save-convert ")
'

printf 'macOS app synthetic smoke passed: %s\n' "$app"
