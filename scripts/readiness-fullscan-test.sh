#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "${MH_SAVE_SYNC_READINESS_TEST_DATABASE_URL:-}" ]]; then
  DATABASE_URL="$MH_SAVE_SYNC_READINESS_TEST_DATABASE_URL" \
  cargo test -p save-server \
    persistent_ready_checks_referenced_objects_after_the_first_2000 \
    -- --ignored --nocapture
  exit 0
fi

if [[ -n "${CONTAINER_RUNTIME:-}" ]]; then
  runtime="$CONTAINER_RUNTIME"
elif command -v podman >/dev/null 2>&1; then
  runtime="podman"
elif command -v docker >/dev/null 2>&1; then
  runtime="docker"
else
  echo "readiness full-scan test requires MH_SAVE_SYNC_READINESS_TEST_DATABASE_URL, podman, or docker" >&2
  exit 1
fi

port="$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
container="mh-save-sync-readiness-test-$$"
cleanup() {
  "$runtime" rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT

"$runtime" run -d --rm \
  --name "$container" \
  -e POSTGRES_USER=mh_test \
  -e POSTGRES_PASSWORD=mh_test \
  -e POSTGRES_DB=mh_test \
  -p "127.0.0.1:${port}:5432" \
  docker.io/library/postgres:17-alpine@sha256:dc17045ccfd343b49600570ea734b9c4991cf1c3f3302e67df51e3b402dd55c4 >/dev/null

for _ in $(seq 1 60); do
  if "$runtime" exec "$container" pg_isready -U mh_test -d mh_test >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
"$runtime" exec "$container" pg_isready -U mh_test -d mh_test >/dev/null

DATABASE_URL="postgres://mh_test:mh_test@127.0.0.1:${port}/mh_test" \
  cargo test -p save-server \
    persistent_ready_checks_referenced_objects_after_the_first_2000 \
    -- --ignored --nocapture
