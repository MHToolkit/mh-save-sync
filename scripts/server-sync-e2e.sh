#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

set +e
port="$(python3 - <<'PY'
import os
import socket
import sys
try:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    print(s.getsockname()[1])
    s.close()
except PermissionError as exc:
    if os.environ.get("MH_SAVE_SYNC_REQUIRE_NETWORK_E2E") == "1":
        raise
    print(
        f"server-sync-e2e skipped: loopback bind denied by sandbox: {exc}",
        file=sys.stderr,
    )
    sys.exit(77)
PY
)"
port_status="$?"
set -e
case "$port_status" in
  0) ;;
  77) exit 0 ;;
  *) exit "$port_status" ;;
esac
server_url="http://127.0.0.1:${port}"
tmp="$(mktemp -d)"
log="${tmp}/server.log"
trap 'status=$?; if [[ -n "${server_pid:-}" ]]; then kill "$server_pid" >/dev/null 2>&1 || true; wait "$server_pid" >/dev/null 2>&1 || true; fi; if [[ $status -ne 0 ]]; then echo "--- mh-save-server log ---" >&2; tail -200 "$log" >&2 || true; fi; rm -rf "$tmp"; exit $status' EXIT

MH_SAVE_SYNC_BIND="127.0.0.1:${port}" cargo run -q -p save-server --bin mh-save-server >"$log" 2>&1 &
server_pid="$!"

for _ in $(seq 1 40); do
  if curl -fsS "${server_url}/ready" >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
curl -fsS "${server_url}/ready" >/dev/null

source_dir="${tmp}/source"
mkdir -p "${source_dir}/slot1"
printf 'office-mac-save-v1' > "${source_dir}/slot1/main.bin"
secret_hex="5555555555555555555555555555555555555555555555555555555555555555"

cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$source_dir" \
  --secret-hex "$secret_hex" \
  --device-id office-mac \
  > "${tmp}/office.json"

python3 - "$server_url" "${tmp}/office.json" <<'PY'
import json
import sys
server_url, path = sys.argv[1:3]
doc = json.load(open(path, encoding="utf-8"))
assert doc["server_url"] == server_url, doc
assert doc["outcome"] == "first-snapshot", doc
assert doc["cloud_head"] == doc["snapshot_id"], doc
assert "已上传到服务器" in doc["message_zh"], doc
PY

printf 'home-android-offline-branch' > "${source_dir}/slot1/main.bin"
cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$source_dir" \
  --secret-hex "$secret_hex" \
  --device-id home-android \
  > "${tmp}/conflict.json"

cargo run -q -p save-cli --bin mh-save -- server-status \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  > "${tmp}/status.json"

python3 - "${tmp}/office.json" "${tmp}/conflict.json" "${tmp}/status.json" <<'PY'
import json
import sys
office = json.load(open(sys.argv[1], encoding="utf-8"))
conflict = json.load(open(sys.argv[2], encoding="utf-8"))
status = json.load(open(sys.argv[3], encoding="utf-8"))
assert conflict["outcome"] == "conflict", conflict
assert conflict["cloud_head"] == office["snapshot_id"], conflict
assert conflict["conflict_snapshot"] == conflict["snapshot_id"], conflict
assert "不会覆盖云端 HEAD" in conflict["message_zh"], conflict
assert status["cloud_head"] == office["snapshot_id"], status
assert status["history_count"] == 2, status
assert status["conflict_count"] == 1, status
assert "云端当前 HEAD" in status["message_zh"], status
print(json.dumps({
    "server_url": status["server_url"],
    "cloud_head": status["cloud_head"],
    "history_count": status["history_count"],
    "conflict_count": status["conflict_count"],
    "evidence": "server sync e2e preserved conflict branch without overwriting cloud head"
}, ensure_ascii=False))
PY
