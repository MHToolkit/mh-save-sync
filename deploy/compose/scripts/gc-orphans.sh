#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
compose_project="${COMPOSE_PROJECT_NAME:-${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync}}"
grace_seconds="${MH_SAVE_SYNC_GC_GRACE_SECONDS:-604800}"
delete=false
physical_only=false

usage() {
  echo "usage: gc-orphans.sh [--grace-seconds N] [--delete|--physical-only]" >&2
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
    --physical-only)
      physical_only=true
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
# MinIO, phase 2 keeps leased keys in a protected ephemeral directory and sends
# the deletion plan over stdin. Keys never enter host compose arguments or the
# final JSON report, and failed mc stderr is replaced with a fixed error.
if [[ "$physical_only" == true ]]; then
  logical_json='{"eligible":0,"deleted":0,"dry_run":false,"physical_purge_pending":0}'
else
  logical_json="$(compose exec -T server /app/mh-save-server "${args[@]}")"
fi
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
purge_tmp="$(mktemp -d)"
chmod 700 "$purge_tmp"
trap 'rm -rf "$purge_tmp"' EXIT
while :; do
  lease_token="$(python3 - <<'PYUUID'
import uuid
print(uuid.uuid4())
PYUUID
)"
  claimed_file="$purge_tmp/claimed"
  versions_file="$purge_tmp/versions"
  plan_file="$purge_tmp/plan"
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
       RETURNING q.storage_key || E'\t' || coalesce(q.head_version_id,'');" \
    >"$claimed_file"
  batch_count="$(python3 - "$claimed_file" <<'PYCOUNT'
import pathlib, sys
print(sum(1 for line in pathlib.Path(sys.argv[1]).read_text().splitlines() if line))
PYCOUNT
)"
  if [[ "$batch_count" == "0" ]]; then
    break
  fi
  compose exec -T minio sh -c '
        set -eu
        user="$(cat /run/secrets/minio_root_user)"
        password="$(cat /run/secrets/minio_root_password)"
        bucket="${S3_BUCKET:-mh-save-sync}"
        mc alias set purge http://127.0.0.1:9000 "$user" "$password" >/dev/null
        mc ls --recursive --versions --json "purge/$bucket/"
      ' >"$versions_file"
  python3 - "$claimed_file" "$versions_file" "$plan_file" <<'PYPLAN'
import json, pathlib, sys

claimed_path, versions_path, plan_path = map(pathlib.Path, sys.argv[1:])
claimed = {}
for line in claimed_path.read_text().splitlines():
    if not line:
        continue
    key, separator, head = line.partition("\t")
    if not separator or not head:
        raise SystemExit("physical purge requires a captured version id")
    claimed[key] = head

versions = {key: [] for key in claimed}
for line in versions_path.read_text().splitlines():
    value = json.loads(line)
    key = value.get("key")
    if key not in versions:
        continue
    version_id = value.get("versionId")
    ordinal = value.get("versionOrdinal")
    if version_id and isinstance(ordinal, int):
        versions[key].append((ordinal, version_id))

with plan_path.open("w") as plan:
    for key, head in claimed.items():
        generations = sorted(versions[key], reverse=True)
        try:
            boundary = next(i for i, (_, version_id) in enumerate(generations) if version_id == head)
        except StopIteration:
            raise SystemExit("captured object version is no longer enumerable") from None
        for _, version_id in generations[boundary:]:
            plan.write(f"{key}\t{version_id}\n")
PYPLAN
  if ! compose exec -T minio sh -c '
      set -eu
      user="$(cat /run/secrets/minio_root_user)"
      password="$(cat /run/secrets/minio_root_password)"
      bucket="${S3_BUCKET:-mh-save-sync}"
      mc alias set purge http://127.0.0.1:9000 "$user" "$password" >/dev/null
      tab="$(printf "\t")"
      while IFS="$tab" read -r key version_id; do
        [ -n "$key" ] || continue
        mc rm --force --version-id "$version_id" "purge/$bucket/$key" >/dev/null
      done
    ' <"$plan_file" >/dev/null 2>"$purge_tmp/delete-error"; then
    echo "physical purge failed" >&2
    exit 70
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
