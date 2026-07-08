#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

real_home="${HOME}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"
export HOME="$tmp/home"
mkdir -p "$HOME"
port="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"
server_url="http://127.0.0.1:${port}"
server_pid=""
log="$tmp/server.log"
trap 'status=$?; if [[ -n "$server_pid" ]]; then kill "$server_pid" >/dev/null 2>&1 || true; wait "$server_pid" >/dev/null 2>&1 || true; fi; rm -rf "$tmp"; exit $status' EXIT

MH_SAVE_SYNC_BIND="127.0.0.1:${port}" cargo run -q -p save-server --bin mh-save-server >"$log" 2>&1 &
server_pid="$!"
for _ in $(seq 1 120); do
  if curl -fsS "$server_url/ready" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready:-0}" != "1" ]]; then
  echo "--- mh-save-server log ---" >&2
  tail -200 "$log" >&2 || true
  curl -fsS "$server_url/ready" >/dev/null
fi

swift run --package-path apps/macos MHSaveSyncMac --set-server-url "$server_url" \
  > "$tmp/set.txt"
grep -q "已保存服务器地址：$server_url" "$tmp/set.txt"


mkdir -p "$HOME/Documents/Secrets" "$tmp/save-root/slot1"
printf "6666666666666666666666666666666666666666666666666666666666666666" > "$HOME/Documents/Secrets/mh-save-sync-test-secret.hex"

swift run --package-path apps/macos MHSaveSyncMac --set-save-root "$tmp/save-root" \
  > "$tmp/set-save-root.txt"
grep -q "已保存 Mac Nemessix 存档目录：$tmp/save-root" "$tmp/set-save-root.txt"

swift run --package-path apps/macos MHSaveSyncMac --set-recovery-secret-file "$HOME/Documents/Secrets/mh-save-sync-test-secret.hex" \
  > "$tmp/set-secret-file.txt"
grep -q "已保存恢复密钥文件路径" "$tmp/set-secret-file.txt"

swift run --package-path apps/macos MHSaveSyncMac --auto-upload-on-exit off \
  > "$tmp/set-auto-off.txt"
grep -q "已关闭：只保留手动同步" "$tmp/set-auto-off.txt"

swift run --package-path apps/macos MHSaveSyncMac --status > "$tmp/status.txt"
grep -q "下一步：启动 MH3G 前点「启动前检查」" "$tmp/status.txt"
grep -q "同步到服务器：$server_url" "$tmp/status.txt"
grep -q "Mac 存档目录：$tmp/save-root" "$tmp/status.txt"
grep -q "恢复密钥文件：$HOME/Documents/Secrets/mh-save-sync-test-secret.hex" "$tmp/status.txt"
grep -q "自动同步：已关闭：只手动同步" "$tmp/status.txt"

swift run --package-path apps/macos MHSaveSyncMac --prelaunch-check \
  > "$tmp/prelaunch.txt"
grep -q "服务器：$server_url" "$tmp/prelaunch.txt"
grep -q "云端连通" "$tmp/prelaunch.txt"
grep -Eq "云端还没有 MH3G 版本|云端已有 MH3G 版本" "$tmp/prelaunch.txt"

swift run --package-path apps/macos MHSaveSyncMac --continue-local \
  > "$tmp/continue-local.txt"
grep -q "已选择继续使用本地存档" "$tmp/continue-local.txt"
grep -q "不会从云端覆盖本地" "$tmp/continue-local.txt"

config_file="$HOME/Library/Application Support/MH Save Sync/config.json"
test -f "$config_file"
python3 - "$config_file" "$server_url" <<'PY'
import json
import sys

config_file, expected = sys.argv[1:3]
doc = json.load(open(config_file, encoding="utf-8"))
assert doc["server_url"] == expected, doc
assert doc["save_root_path"].endswith("/save-root"), doc
assert doc["recovery_secret_file"].endswith("/Documents/Secrets/mh-save-sync-test-secret.hex"), doc
assert doc["auto_upload_on_exit"] is False, doc
PY

python3 - "$server_url" <<'PY'
import json
import sys

print(json.dumps({
    "macos_config_e2e": True,
    "server_url": sys.argv[1],
    "continue_local_visible": True,
    "config_path": "~/Library/Application Support/MH Save Sync/config.json",
    "save_root_configured": True,
    "recovery_secret_file_configured": True,
    "auto_upload_on_exit": False,
}, ensure_ascii=False, sort_keys=True))
PY
