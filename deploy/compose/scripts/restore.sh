#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
backup_dir="${1:?backup directory required}"

compose() {
  "$runtime" compose --env-file "$env_file" -f "$compose_file" "$@"
}

shasum -a 256 -c "$backup_dir/SHA256SUMS"
compose down
"$runtime" volume rm mh-save-sync_postgres-data mh-save-sync_minio-data 2>/dev/null || true
compose up -d postgres --wait
compose exec -T postgres psql -v ON_ERROR_STOP=1 -U mh_save_sync -d mh_save_sync \
  < "$backup_dir/postgres.sql"
compose create minio >/dev/null
"$runtime" run --rm \
  -i \
  -v mh-save-sync_minio-data:/data \
  postgres:17-alpine \
  tar -C /data -xf - < "$backup_dir/minio-data.tar"
compose up -d --wait
COMPOSE_ENV_FILE="$env_file" CONTAINER_RUNTIME="$runtime" \
  "$script_dir/verify-repository.sh"
