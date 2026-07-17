#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
compose_project="${COMPOSE_PROJECT_NAME:-${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync}}"
postgres_volume="${MH_SAVE_SYNC_POSTGRES_VOLUME:-${compose_project}_postgres-data}"
minio_volume="${MH_SAVE_SYNC_MINIO_VOLUME:-${compose_project}_minio-data}"
backup_dir="${1:?backup directory required}"

compose() {
  "$runtime" compose --project-name "$compose_project" --env-file "$env_file" -f "$compose_file" "$@"
}

shasum -a 256 -c "$backup_dir/SHA256SUMS"
compose down
"$runtime" volume rm "$postgres_volume" "$minio_volume" 2>/dev/null || true
compose up -d postgres --wait
compose exec -T postgres psql -v ON_ERROR_STOP=1 -U mh_save_sync -d mh_save_sync \
  < "$backup_dir/postgres.sql"
compose create minio >/dev/null
"$runtime" run --rm \
  -i \
  -v "$minio_volume:/data" \
  postgres:17-alpine \
  tar -C /data -xf - < "$backup_dir/minio-data.tar"
compose up -d --wait
COMPOSE_ENV_FILE="$env_file" CONTAINER_RUNTIME="$runtime" COMPOSE_PROJECT_NAME="$compose_project" \
  "$script_dir/verify-repository.sh"
