#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "${MH_SAVE_SYNC_GC_TEST_DATABASE_URL:-}" ]]; then
  for test_name in \
    orphan_gc_is_account_scoped_and_preserves_live_references \
    commit_failpoints_prove_transactional_head_and_orphan_recovery \
    slow_delete_lock_does_not_block_unrelated_foreground_object
  do
    DATABASE_URL="$MH_SAVE_SYNC_GC_TEST_DATABASE_URL" \
      cargo test -p save-server "$test_name" -- --ignored --nocapture
  done
  exit 0
fi

if [[ -n "${CONTAINER_RUNTIME:-}" ]]; then
  runtime="$CONTAINER_RUNTIME"
elif command -v podman >/dev/null 2>&1; then
  runtime="podman"
elif command -v docker >/dev/null 2>&1; then
  runtime="docker"
else
  echo "server crash/GC test requires MH_SAVE_SYNC_GC_TEST_DATABASE_URL, podman, or docker" >&2
  exit 1
fi

port="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
container="mh-save-sync-gc-test-$$"
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

for test_name in \
  orphan_gc_is_account_scoped_and_preserves_live_references \
  commit_failpoints_prove_transactional_head_and_orphan_recovery \
  slow_delete_lock_does_not_block_unrelated_foreground_object
do
  DATABASE_URL="postgres://mh_test:mh_test@127.0.0.1:${port}/mh_test" \
    cargo test -p save-server "$test_name" -- --ignored --nocapture
done
