#!/usr/bin/env bash
set -euo pipefail
backup_dir="${1:?backup directory required}"
shasum -a 256 -c "$backup_dir/SHA256SUMS"
docker compose down
docker volume rm mh-save-sync_postgres-data mh-save-sync_minio-data 2>/dev/null || true
docker compose up -d postgres minio --wait
docker compose exec -T postgres psql -U mh_save_sync -d mh_save_sync < "$backup_dir/postgres.sql"
docker compose exec -T minio sh -c 'cd /data && tar -xf -' < "$backup_dir/minio-data.tar"
docker compose up -d --wait
./scripts/verify-repository.sh
