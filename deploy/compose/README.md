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

For a public self-hosted endpoint, run the API behind the optional Caddy TLS
reverse proxy and keep the direct API port private to the host. DNS for
`MH_SAVE_SYNC_PUBLIC_HOST` must already point at the server, and ports 80/443
must reach the proxy. MinIO ports remain internal/admin-only; clients use only
`https://<host>/`.

```bash
cat >> "$HOME/Documents/Secrets/mh-save-sync.env" <<'EOF'
MH_SAVE_SYNC_PUBLIC_HOST="save.example.com"
MH_SAVE_SYNC_HTTP_PORT="127.0.0.1:18082"
MH_SAVE_SYNC_PUBLIC_HTTP_PORT="80"
MH_SAVE_SYNC_PUBLIC_HTTPS_PORT="443"
EOF

docker compose \
  --env-file "$HOME/Documents/Secrets/mh-save-sync.env" \
  -f deploy/compose/compose.yaml \
  -f deploy/compose/compose.tls.yaml up -d --wait
curl -fsS https://save.example.com/ready
```

On low-resource hosts, leave the Rust API exposed only on loopback and tune the
healthcheck intervals instead of increasing runner/job concurrency:

```bash
MH_SAVE_SYNC_DB_HEALTH_INTERVAL="15s"
MH_SAVE_SYNC_MINIO_HEALTH_INTERVAL="15s"
MH_SAVE_SYNC_SERVER_HEALTH_INTERVAL="30s"
MH_SAVE_SYNC_TLS_HEALTH_INTERVAL="30s"
```

This stack is for isolated development and disaster-recovery testing. It must not be deployed over `nemessix-room`; use a distinct Compose project name and non-conflicting ports.

Backups require both PostgreSQL and MinIO object data. Restoring only one side must be treated as not ready until `scripts/verify-repository.sh` proves every committed snapshot references durable encrypted objects.

Podman users can run the same scripts with:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
deploy/compose/scripts/backup.sh
```

Destructive restore:

```bash
CONTAINER_RUNTIME=podman \
COMPOSE_PROJECT_NAME=mh-save-sync-aliyun \
COMPOSE_ENV_FILE="$HOME/Documents/Secrets/mh-save-sync.env" \
deploy/compose/scripts/restore.sh "$HOME/Games/Backups/MHSaveSync/<run>"
```

The backup script stops only the API writer, dumps PostgreSQL, and archives the
immutable MinIO volume. It restarts the API before returning. Restore verifies
artifact checksums, recreates both volumes, restores both stores, waits for
health, and checks for dangling snapshot-object references.

Orphan collection is dry-run by default. PostgreSQL snapshot references and
unexpired upload sessions are the reachability truth. Destructive runs use a
recoverable mark/lease row and an account/object-scoped PostgreSQL advisory lock;
slow S3 deletion does not lock the global upload or snapshot tables. Output
contains aggregate counts only.

Because this Compose stack enables MinIO versioning, collection is explicitly
two-stage. The Rust server removes current objects and durably queues opaque
keys; the Compose wrapper then uses `mc rm --versions` over stdin to purge every
noncurrent version and delete marker before acknowledging the queue lease.
`physical_purge_pending` must be zero before claiming storage was physically
reclaimed. A non-MinIO S3 deployment must provide equivalent bucket lifecycle
or provider-specific version purge processing.

Preview objects older than the default seven-day grace period:

    deploy/compose/scripts/gc-orphans.sh

Run an explicit destructive sweep with a 24-hour grace period:

    deploy/compose/scripts/gc-orphans.sh --grace-seconds 86400 --delete
