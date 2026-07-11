#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

normalize_url() {
  python3 - "$1" <<'PY'
import sys
print(sys.argv[1].rstrip("/"))
PY
}

free_ports() {
  python3 - <<'PY'
import socket

sockets = []
try:
    for _ in range(3):
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        sockets.append(sock)
    print(" ".join(str(sock.getsockname()[1]) for sock in sockets))
finally:
    for sock in sockets:
        sock.close()
PY
}

runtime="${CONTAINER_RUNTIME:-}"
compose_file="${COMPOSE_FILE:-$repo_root/deploy/compose/compose.yaml}"
compose_project="${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync-persistent-e2e-$$}"
server_url="${MH_SAVE_SYNC_SERVER_URL:-}"
started_compose=0
tmp="$(mktemp -d)"
secrets_dir=""
env_file=""

compose() {
  "$runtime" compose \
    --project-name "$compose_project" \
    --env-file "$env_file" \
    -f "$compose_file" \
    "$@"
}

runtime_usable() {
  local candidate="$1"

  case "$candidate" in
    docker)
      command -v docker >/dev/null 2>&1 || return 1
      docker compose version >/dev/null 2>&1 || return 1
      docker info >/dev/null 2>&1 || return 1
      ;;
    podman)
      command -v podman >/dev/null 2>&1 || return 1
      podman info >/dev/null 2>&1 || return 1
      podman compose version >/dev/null 2>&1 || return 1
      ;;
    *)
      return 2
      ;;
  esac
}

select_runtime() {
  local explicit="$1"

  if [[ -n "$explicit" ]]; then
    if [[ "$explicit" != "docker" && "$explicit" != "podman" ]]; then
      blocked "CONTAINER_RUNTIME=$explicit is unsupported; expected docker or podman"
    fi
    if runtime_usable "$explicit"; then
      printf '%s\n' "$explicit"
      return
    fi
    blocked "CONTAINER_RUNTIME=$explicit is not usable; verify '$explicit info' and '$explicit compose version'"
  fi

  if runtime_usable docker; then
    printf '%s\n' docker
    return
  fi
  if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
    echo "INFO: docker compose is installed, but docker daemon is not usable; trying podman" >&2
  fi

  if runtime_usable podman; then
    printf '%s\n' podman
    return
  fi

  blocked "Docker Compose or Podman Compose with a usable runtime daemon is required to start deploy/compose/compose.yaml"
}

cleanup() {
  status=$?
  if [[ $status -ne 0 && $started_compose -eq 1 ]]; then
    echo "--- compose ps ---" >&2
    compose ps >&2 || true
    echo "--- compose server logs ---" >&2
    compose logs --no-color --tail=200 server >&2 || true
  fi
  if [[ $started_compose -eq 1 ]]; then
    compose down -v --remove-orphans >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
  if [[ -n "$secrets_dir" ]]; then
    rm -rf "$secrets_dir"
  fi
  exit "$status"
}
trap cleanup EXIT

if [[ -n "$server_url" ]]; then
  server_url="$(normalize_url "$server_url")"
else
  runtime="$(select_runtime "$runtime")"

  if [[ "${MH_SAVE_SYNC_RUNTIME_PROBE_ONLY:-0}" == "1" ]]; then
    python3 - "$runtime" <<'PY'
import json
import sys

print(json.dumps({"selected_runtime": sys.argv[1]}, sort_keys=True))
PY
    exit 0
  fi

  read -r discovered_http discovered_minio_api discovered_minio_console < <(free_ports)
  http_port="${MH_SAVE_SYNC_HTTP_PORT:-$discovered_http}"
  minio_api_port="${MH_SAVE_SYNC_MINIO_API_PORT:-$discovered_minio_api}"
  minio_console_port="${MH_SAVE_SYNC_MINIO_CONSOLE_PORT:-$discovered_minio_console}"

  secrets_parent="${MH_SAVE_SYNC_E2E_SECRETS_PARENT:-$HOME/Documents/Secrets}"
  mkdir -p "$secrets_parent"
  secrets_dir="$(mktemp -d "$secrets_parent/mh-save-sync-compose-e2e.XXXXXX")"
  chmod 700 "$secrets_dir"
  env_file="$secrets_dir/compose.env"
  old_umask="$(umask)"
  umask 077
  openssl rand -hex 32 > "$secrets_dir/postgres_password.txt"
  printf 'mh-save-sync-e2e' > "$secrets_dir/minio_root_user.txt"
  openssl rand -hex 32 > "$secrets_dir/minio_root_password.txt"
  cat > "$env_file" <<EOF
MH_SAVE_SYNC_SECRETS_DIR="$secrets_dir"
MH_SAVE_SYNC_HTTP_PORT="$http_port"
MH_SAVE_SYNC_MINIO_API_PORT="$minio_api_port"
MH_SAVE_SYNC_MINIO_CONSOLE_PORT="$minio_console_port"
EOF
  umask "$old_umask"

  compose down -v --remove-orphans >/dev/null 2>&1 || true
  compose up -d --build --wait
  started_compose=1
  server_url="http://127.0.0.1:$http_port"
fi

ready_json="$tmp/ready.json"
for _ in $(seq 1 120); do
  if curl -fsS "$server_url/ready" > "$ready_json" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.5
done
if [[ "${ready:-0}" != "1" ]]; then
  curl -fsS "$server_url/ready" > "$ready_json"
fi

python3 - "$ready_json" <<'PY'
import json
import sys

ready = json.load(open(sys.argv[1], encoding="utf-8"))
if ready.get("backend") != "postgres-s3":
    raise SystemExit(f"persistent postgres-s3 backend required, got {ready}")
PY

run_id="$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)"
logical_save_id="compose-cli-${run_id}"
secret_hex="4242424242424242424242424242424242424242424242424242424242424242"

office_dir="$tmp/office-macos"
home_dir="$tmp/home-android"
restore_dir="$tmp/restored-head"
mkdir -p "$office_dir/slot1" "$home_dir/slot1"
printf 'office-macos-postgres-s3-head-v1' > "$office_dir/slot1/main.bin"
printf 'home-android-postgres-s3-divergent-branch' > "$home_dir/slot1/main.bin"

cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$office_dir" \
  --secret-hex "$secret_hex" \
  --device-id office-mac \
  --logical-save-id "$logical_save_id" \
  > "$tmp/office.json"

cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$home_dir" \
  --secret-hex "$secret_hex" \
  --device-id home-android \
  --logical-save-id "$logical_save_id" \
  > "$tmp/conflict.json"

cargo run -q -p save-cli --bin mh-save -- server-status \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  > "$tmp/status.json"

cargo run -q -p save-cli --bin mh-save -- server-restore \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  --target "$restore_dir" \
  --emulator-state stopped \
  > "$tmp/restore.json"

if cargo run -q -p save-cli --bin mh-save -- server-restore \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  --target "$tmp/blocked-running" \
  --emulator-state running \
  > "$tmp/blocked.json" 2> "$tmp/blocked.err"; then
  echo "running persistent server restore unexpectedly succeeded" >&2
  exit 1
fi
grep -q "已拒绝恢复：模拟器仍在运行，没有覆盖本地存档" "$tmp/blocked.err"
if [[ -e "$tmp/blocked-running" ]]; then
  echo "running persistent server restore created target directory" >&2
  exit 1
fi

python3 - \
  "$ready_json" \
  "$server_url" \
  "$tmp/office.json" \
  "$tmp/conflict.json" \
  "$tmp/status.json" \
  "$tmp/restore.json" \
  "$office_dir" \
  "$restore_dir" <<'PY'
import json
import pathlib
import sys

ready_path, server_url, office_path, conflict_path, status_path, restore_path, office_dir, restore_dir = sys.argv[1:9]
ready = json.load(open(ready_path, encoding="utf-8"))
office = json.load(open(office_path, encoding="utf-8"))
conflict = json.load(open(conflict_path, encoding="utf-8"))
status = json.load(open(status_path, encoding="utf-8"))
restore = json.load(open(restore_path, encoding="utf-8"))
office_dir = pathlib.Path(office_dir)
restore_dir = pathlib.Path(restore_dir)

assert ready["backend"] == "postgres-s3", ready
assert office["server_url"] == server_url, office
assert office["outcome"] == "first-snapshot", office
assert office["cloud_head"] == office["snapshot_id"], office
assert conflict["outcome"] == "conflict", conflict
assert conflict["cloud_head"] == office["snapshot_id"], conflict
assert conflict["conflict_snapshot"] == conflict["snapshot_id"], conflict
assert status["server_url"] == server_url, status
assert status["cloud_head"] == office["snapshot_id"], status
assert status["history_count"] == 2, status
assert status["conflict_count"] == 1, status
assert restore["snapshot_id"] == office["snapshot_id"], restore
restored = restore_dir / "slot1" / "main.bin"
expected = office_dir / "slot1" / "main.bin"
assert restored.read_bytes() == expected.read_bytes(), (restored, expected)

print(json.dumps({
    "backend": ready["backend"],
    "server_url": server_url,
    "logical_save_id": status["logical_save_id"],
    "cloud_head": status["cloud_head"],
    "history_count": status["history_count"],
    "conflict_count": status["conflict_count"],
    "restored_snapshot_id": restore["snapshot_id"],
    "running_restore_fail_closed": True,
    "evidence": "persistent postgres-s3 server-upload/status/server-restore preserved conflict branch and restored byte-identical cloud HEAD"
}, ensure_ascii=False, sort_keys=True))
PY
