# MH Save Sync self-hosted compose

Five-minute local demo:

```bash
secret_dir="$HOME/Documents/Secrets/mh-save-sync-compose"
mkdir -p "$secret_dir"
openssl rand -hex 32 > "$secret_dir/postgres_password.txt"
printf 'mh-save-sync-local' > "$secret_dir/minio_root_user.txt"
openssl rand -hex 32 > "$secret_dir/minio_root_password.txt"
chmod 600 "$secret_dir"/*.txt
cat > "$HOME/Documents/Secrets/mh-save-sync.env" <<EOF
MH_SAVE_SYNC_SECRETS_DIR="$secret_dir"
EOF
chmod 600 "$HOME/Documents/Secrets/mh-save-sync.env"
docker compose \
  --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml up -d --wait
curl -fsS http://127.0.0.1:18080/ready
```

Compose starts a one-shot `minio-init` container that creates the configured
bucket and enables MinIO versioning before the API can become healthy. The API
uses S3 SHA-256 upload checksums and readiness verifies that every committed
snapshot object still exists.

For isolated hosts that already use the default ports, set these quoted values
in the env file before starting Compose:

```bash
MH_SAVE_SYNC_HTTP_PORT="18082"
MH_SAVE_SYNC_MINIO_API_PORT="19082"
MH_SAVE_SYNC_MINIO_CONSOLE_PORT="19083"
```

This stack is for isolated development and disaster-recovery testing. It must not be deployed over `nemessix-room`; use a distinct Compose project name and non-conflicting ports.

Backups require both PostgreSQL and MinIO object data. Restoring only one side must be treated as not ready until `scripts/verify-repository.sh` proves every committed snapshot references durable encrypted objects.

Podman users can run the same scripts with:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
deploy/compose/scripts/backup.sh
```

Destructive restore:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
deploy/compose/scripts/restore.sh "$HOME/Games/Backups/MHSaveSync/<run>"
```

The backup script stops only the API writer, dumps PostgreSQL, and archives the
immutable MinIO volume. It restarts the API before returning. Restore verifies
artifact checksums, recreates both volumes, restores both stores, waits for
health, and checks for dangling snapshot-object references.
