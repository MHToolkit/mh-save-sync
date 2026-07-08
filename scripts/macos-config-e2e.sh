#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

export HOME="$tmp/home"
mkdir -p "$HOME"
server_url="http://127.0.0.1:39082"

swift run --package-path apps/macos MHSaveSyncMac --set-server-url "$server_url" \
  > "$tmp/set.txt"
grep -q "已保存服务器地址：$server_url" "$tmp/set.txt"

swift run --package-path apps/macos MHSaveSyncMac --status > "$tmp/status.txt"
grep -q "同步到服务器：$server_url" "$tmp/status.txt"

swift run --package-path apps/macos MHSaveSyncMac --prelaunch-check \
  > "$tmp/prelaunch.txt"
grep -q "服务器：$server_url" "$tmp/prelaunch.txt"

config_file="$HOME/Library/Application Support/MH Save Sync/config.json"
test -f "$config_file"
python3 - "$config_file" "$server_url" <<'PY'
import json
import sys

config_file, expected = sys.argv[1:3]
doc = json.load(open(config_file, encoding="utf-8"))
assert doc["server_url"] == expected, doc
PY

python3 - "$server_url" <<'PY'
import json
import sys

print(json.dumps({
    "macos_config_e2e": True,
    "server_url": sys.argv[1],
    "config_path": "~/Library/Application Support/MH Save Sync/config.json",
}, ensure_ascii=False, sort_keys=True))
PY
