#!/usr/bin/env bash
set -euo pipefail
out="${1:-$HOME/Games/Backups/MHSaveSync/$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out"
docker compose exec -T postgres pg_dump -U mh_save_sync mh_save_sync > "$out/postgres.sql"
docker compose exec -T minio sh -c 'cd /data && tar -cf - .' > "$out/minio-data.tar"
shasum -a 256 "$out/postgres.sql" "$out/minio-data.tar" > "$out/SHA256SUMS"
echo "$out"
