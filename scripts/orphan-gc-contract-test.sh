#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if grep -R -q 'MH_SAVE_SYNC_TEST_COMMIT_FAILPOINT' "$repo_root/crates/save-server"; then
  echo "runtime commit failpoint leaked into production source" >&2
  exit 1
fi
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"
: > "$tmp/env"

cat > "$tmp/bin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "${MH_SAVE_SYNC_FAKE_RUNTIME_LOG:?}"
if [[ "$*" == *"exec -T server /app/mh-save-server"* ]]; then
  printf '{"eligible":2,"deleted":0,"dry_run":true,"physical_purge_pending":0}\n'
elif [[ "$*" == *"exec -T minio sh -c"* ]]; then
  cat >/dev/null
  printf '0\n'
elif [[ "$*" == *"SELECT count(*) FROM orphan_gc_purge_queue"* ]]; then
  printf '0\n'
fi
SH
chmod +x "$tmp/bin/docker"

log="$tmp/runtime.log"
PATH="$tmp/bin:$PATH" \
MH_SAVE_SYNC_FAKE_RUNTIME_LOG="$log" \
CONTAINER_RUNTIME=docker \
COMPOSE_PROJECT_NAME=gc-fixture \
COMPOSE_ENV_FILE="$tmp/env" \
"$repo_root/deploy/compose/scripts/gc-orphans.sh" --grace-seconds 3600 >/dev/null

grep -q -- '--project-name gc-fixture' "$log"
grep -q -- '/app/mh-save-server --gc-orphans --grace-seconds 3600' "$log"
if grep -q -- '--delete' "$log"; then
  echo "dry-run unexpectedly requested deletion" >&2
  exit 1
fi

: > "$log"
PATH="$tmp/bin:$PATH" \
MH_SAVE_SYNC_FAKE_RUNTIME_LOG="$log" \
CONTAINER_RUNTIME=docker \
COMPOSE_PROJECT_NAME=gc-fixture \
COMPOSE_ENV_FILE="$tmp/env" \
"$repo_root/deploy/compose/scripts/gc-orphans.sh" --grace-seconds 3600 --delete >/dev/null
grep -q -- '--delete' "$log"

if PATH="$tmp/bin:$PATH" "$repo_root/deploy/compose/scripts/gc-orphans.sh" --grace-seconds 0 >/dev/null 2>&1; then
  echo "zero grace unexpectedly accepted" >&2
  exit 1
fi

echo "orphan GC shell contract: PASS"
