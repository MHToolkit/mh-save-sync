#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
compose_project="${COMPOSE_PROJECT_NAME:-${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync}}"
grace_seconds="${MH_SAVE_SYNC_GC_GRACE_SECONDS:-604800}"
delete=false

usage() {
  echo "usage: gc-orphans.sh [--grace-seconds N] [--delete]" >&2
}

while (($#)); do
  case "$1" in
    --grace-seconds)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      grace_seconds="$2"
      shift 2
      ;;
    --delete)
      delete=true
      shift
      ;;
    *)
      usage
      exit 64
      ;;
  esac
done

if [[ ! "$grace_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "grace seconds must be a positive integer" >&2
  exit 64
fi

compose() {
  "$runtime" compose --project-name "$compose_project" --env-file "$env_file" -f "$compose_file" "$@"
}

args=(--gc-orphans --grace-seconds "$grace_seconds")
if [[ "$delete" == true ]]; then
  args+=(--delete)
fi

# Phase 1 removes current objects and queues opaque storage keys. On versioned
# MinIO, phase 2 consumes those keys only over stdin and removes every version.
# Keys never appear in command arguments, stdout, or the final JSON report.
logical_json="$(compose exec -T server /app/mh-save-server "${args[@]}")"
if [[ "$delete" != true ]]; then
  python3 - "$logical_json" <<'PYJSON'
import json, sys
value = json.loads(sys.argv[1])
value["physical_purged"] = 0
print(json.dumps(value, separators=(",", ":")))
PYJSON
  exit 0
fi

physical_purged=0
while :; do
  lease_token="$(python3 - <<'PYUUID'
import uuid
print(uuid.uuid4())
PYUUID
)"
  batch_count="$(
    compose exec -T postgres psql -qAt -v ON_ERROR_STOP=1 \
      -U mh_save_sync -d mh_save_sync -c \
      "WITH claimed AS (
         SELECT account_handle,storage_key
         FROM orphan_gc_purge_queue
         WHERE lease_until IS NULL OR lease_until<now()
         ORDER BY queued_at
         LIMIT 1000
         FOR UPDATE SKIP LOCKED
       )
       UPDATE orphan_gc_purge_queue q
       SET lease_token='$lease_token',lease_until=now()+interval '5 minutes'
       FROM claimed c
       WHERE q.account_handle=c.account_handle AND q.storage_key=c.storage_key
       RETURNING q.storage_key;" |
      compose exec -T minio sh -c '
        set -eu
        user="$(cat /run/secrets/minio_root_user)"
        password="$(cat /run/secrets/minio_root_password)"
        bucket="${S3_BUCKET:-mh-save-sync}"
        mc alias set purge http://127.0.0.1:9000 "$user" "$password" >/dev/null
        count=0
        while IFS= read -r key; do
          [ -n "$key" ] || continue
          mc rm --force --versions "purge/$bucket/$key" >/dev/null
          count=$((count + 1))
        done
        printf "%s\n" "$count"
      '
  )"
  [[ "$batch_count" =~ ^[0-9]+$ ]]
  if [[ "$batch_count" == "0" ]]; then
    break
  fi
  compose exec -T postgres psql -qAt -v ON_ERROR_STOP=1 \
    -U mh_save_sync -d mh_save_sync -c \
    "DELETE FROM orphan_gc_purge_queue WHERE lease_token='$lease_token';" >/dev/null
  physical_purged=$((physical_purged + batch_count))
done

pending="$(compose exec -T postgres psql -qAt -v ON_ERROR_STOP=1 \
  -U mh_save_sync -d mh_save_sync -c \
  "SELECT count(*) FROM orphan_gc_purge_queue;")"
python3 - "$logical_json" "$physical_purged" "$pending" <<'PYJSON'
import json, sys
value = json.loads(sys.argv[1])
value["physical_purged"] = int(sys.argv[2])
value["physical_purge_pending"] = int(sys.argv[3])
print(json.dumps(value, separators=(",", ":")))
PYJSON
