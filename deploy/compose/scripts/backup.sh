#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
out="${1:-$HOME/Games/Backups/MHSaveSync/$(date +%Y%m%d-%H%M%S)}"

compose() {
  "$runtime" compose --env-file "$env_file" -f "$compose_file" "$@"
}

mkdir -p "$out"
compose stop server >/dev/null
trap 'compose start server >/dev/null 2>&1 || true' EXIT

# PostgreSQL is dumped before the immutable object volume. With writers stopped,
# HEAD can never reference an object absent from this backup.
compose exec -T postgres pg_dump -U mh_save_sync mh_save_sync > "$out/postgres.sql"
"$runtime" run --rm \
  -v mh-save-sync_minio-data:/data:ro \
  postgres:17-alpine \
  tar -C /data -cf - . > "$out/minio-data.tar"
shasum -a 256 "$out/postgres.sql" "$out/minio-data.tar" > "$out/SHA256SUMS"
compose start server >/dev/null
trap - EXIT
echo "$out"
