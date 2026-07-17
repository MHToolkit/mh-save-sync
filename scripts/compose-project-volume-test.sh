#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fakebin="$tmp/bin"
backup_dir="$tmp/backup"
mkdir -p "$fakebin" "$backup_dir"
: > "$tmp/env"
printf 'hash  %s\n' "$backup_dir/postgres.sql" > "$backup_dir/SHA256SUMS"
printf 'postgres' > "$backup_dir/postgres.sql"
printf 'minio' > "$backup_dir/minio-data.tar"

cat > "$fakebin/shasum" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "-a" && "${2:-}" == "256" && "${3:-}" == "-c" ]]; then
  echo "$4: OK"
  exit 0
fi
exec /usr/bin/shasum "$@"
SH

cat > "$fakebin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
log="${MH_SAVE_SYNC_FAKE_RUNTIME_LOG:?}"
printf '%s\n' "$*" >> "$log"
if [[ "${1:-}" == "compose" ]]; then
  shift
  if [[ "$*" != *"--project-name mh-save-sync-aliyun"* ]]; then
    echo "missing project name in compose call: $*" >&2
    exit 64
  fi
  case "${*: -1}" in
    server|postgres|minio) exit 0 ;;
  esac
  if [[ "$*" == *" exec -T postgres pg_dump "* ]]; then
    printf 'fake-postgres-dump'
    exit 0
  fi
  if [[ "$*" == *" exec -T postgres psql "* ]]; then
    cat >/dev/null
    exit 0
  fi
  if [[ "$*" == *" exec -T postgres psql -U mh_save_sync"* ]]; then
    echo 'dangling_snapshot_objects=0'
    exit 0
  fi
  exit 0
fi
if [[ "${1:-}" == "run" ]]; then
  if [[ "$*" == *"mh-save-sync-aliyun_minio-data"* ]]; then
    if [[ "$*" == *":/data:ro"* ]]; then
      printf 'fake-minio-tar'
    else
      cat >/dev/null
    fi
    exit 0
  fi
  echo "missing isolated minio volume in runtime call: $*" >&2
  exit 65
fi
if [[ "${1:-}" == "volume" && "${2:-}" == "rm" ]]; then
  if [[ "$*" == *"mh-save-sync-aliyun_postgres-data"* && "$*" == *"mh-save-sync-aliyun_minio-data"* ]]; then
    exit 0
  fi
  echo "missing isolated volume names in volume rm: $*" >&2
  exit 66
fi
echo "unexpected docker invocation: $*" >&2
exit 67
SH

cat > "$fakebin/curl" <<'SH'
#!/usr/bin/env bash
echo '{"status":"ready","version":"0.1.0","backend":"postgres-s3"}'
SH
chmod +x "$fakebin/docker" "$fakebin/curl" "$fakebin/shasum"

log="$tmp/runtime.log"
PATH="$fakebin:$PATH" \
MH_SAVE_SYNC_FAKE_RUNTIME_LOG="$log" \
CONTAINER_RUNTIME=docker \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$tmp/env" \
"$repo_root/deploy/compose/scripts/backup.sh" "$tmp/out" >/tmp/mh-save-sync-backup-test.out

PATH="$fakebin:$PATH" \
MH_SAVE_SYNC_FAKE_RUNTIME_LOG="$log" \
CONTAINER_RUNTIME=docker \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$tmp/env" \
MH_SAVE_SYNC_PUBLIC_PORT=18082 \
"$repo_root/deploy/compose/scripts/restore.sh" "$backup_dir" >/tmp/mh-save-sync-restore-test.out

if grep -q 'mh-save-sync_minio-data\|mh-save-sync_postgres-data' "$log"; then
  echo "default project volume leaked into compose project-aware scripts" >&2
  cat "$log" >&2
  exit 1
fi
if ! grep -q -- '--project-name mh-save-sync-aliyun' "$log"; then
  echo "compose project name was not passed to runtime" >&2
  cat "$log" >&2
  exit 1
fi
if ! grep -q 'mh-save-sync-aliyun_minio-data' "$log" || ! grep -q 'mh-save-sync-aliyun_postgres-data' "$log"; then
  echo "isolated project volumes were not used" >&2
  cat "$log" >&2
  exit 1
fi

printf '{"compose_project_volume_test":true,"project":"mh-save-sync-aliyun","isolated_volumes":true}\n'
