#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
compose_project="${COMPOSE_PROJECT_NAME:-${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync}}"
public_port="${MH_SAVE_SYNC_PUBLIC_PORT:-18080}"

compose() {
  "$runtime" compose --project-name "$compose_project" --env-file "$env_file" -f "$compose_file" "$@"
}

curl -fsS "http://127.0.0.1:${public_port}/ready"
printf '\n'
compose exec -T postgres psql -U mh_save_sync -d mh_save_sync -Atc \
  "SELECT 'dangling_snapshot_objects=' || count(*)
   FROM snapshot_objects so
   LEFT JOIN objects o
     ON o.account_handle=so.account_handle AND o.object_id=so.object_id
   WHERE o.object_id IS NULL;"
