#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build -q -p save-cli --bin mh-save
swift build --package-path apps/macos >/dev/null

port="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"
server_url="http://127.0.0.1:${port}"
tmp="$(mktemp -d)"
log="${tmp}/server.log"
trap 'status=$?; if [[ -n "${server_pid:-}" ]]; then kill "$server_pid" >/dev/null 2>&1 || true; wait "$server_pid" >/dev/null 2>&1 || true; fi; if [[ $status -ne 0 ]]; then echo "--- mh-save-server log ---" >&2; tail -200 "$log" >&2 || true; fi; rm -rf "$tmp"; exit $status' EXIT

MH_SAVE_SYNC_BIND="127.0.0.1:${port}" cargo run -q -p save-server --bin mh-save-server >"$log" 2>&1 &
server_pid="$!"
for _ in $(seq 1 120); do
  if curl -fsS "${server_url}/ready" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready:-0}" != "1" ]]; then
  curl -fsS "${server_url}/ready" >/dev/null
fi

source_dir="${tmp}/source"
restore_dir="${tmp}/restored-by-macos-shell"
mkdir -p "${source_dir}/slot1"
printf 'macos-shell-visible-save' > "${source_dir}/slot1/main.bin"
secret_hex="6666666666666666666666666666666666666666666666666666666666666666"
export MH_SAVE_SYNC_SERVER_URL="$server_url"
export MH_SAVE_SYNC_CLI="$repo_root/target/debug/mh-save"

swift run --package-path apps/macos MHSaveSyncMac --server-upload \
  --root "$source_dir" \
  --secret-hex "$secret_hex" \
  > "${tmp}/mac-upload.txt"

grep -q "已上传到服务器" "${tmp}/mac-upload.txt"
grep -q "$server_url" "${tmp}/mac-upload.txt"

swift run --package-path apps/macos MHSaveSyncMac --server-status \
  --secret-hex "$secret_hex" \
  > "${tmp}/mac-status.txt"

grep -q "云端当前 HEAD" "${tmp}/mac-status.txt"

swift run --package-path apps/macos MHSaveSyncMac --server-restore \
  --target "$restore_dir" \
  --secret-hex "$secret_hex" \
  --emulator-state stopped \
  > "${tmp}/mac-restore.txt"

grep -q "已从服务器下载并恢复" "${tmp}/mac-restore.txt"
test "$(cat "${restore_dir}/slot1/main.bin")" = "macos-shell-visible-save"

if swift run --package-path apps/macos MHSaveSyncMac --server-restore \
  --target "${tmp}/blocked-running" \
  --secret-hex "$secret_hex" \
  --emulator-state running \
  > "${tmp}/mac-blocked.txt" 2>&1; then
  echo "macOS shell running restore unexpectedly succeeded" >&2
  exit 1
fi
grep -q "restore refused while emulator is running" "${tmp}/mac-blocked.txt"

python3 - "$server_url" "${tmp}/mac-upload.txt" "${tmp}/mac-status.txt" "${tmp}/mac-restore.txt" <<'PY'
import json
import sys
server_url, upload, status, restore = sys.argv[1:5]
print(json.dumps({
    "server_url": server_url,
    "macos_shell_upload_visible": "已上传到服务器" in open(upload, encoding="utf-8").read(),
    "macos_shell_status_visible": "云端当前 HEAD" in open(status, encoding="utf-8").read(),
    "macos_shell_restore_visible": "已从服务器下载并恢复" in open(restore, encoding="utf-8").read(),
    "running_restore_fail_closed": True,
}, ensure_ascii=False))
PY
