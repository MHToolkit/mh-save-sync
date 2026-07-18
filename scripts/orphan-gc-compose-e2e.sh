#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
runtime="${CONTAINER_RUNTIME:-docker}"
project="mh-save-sync-gc-e2e-$$"
tmp="$(mktemp -d)"
secret_dir="$tmp/secrets"
mkdir -p "$secret_dir"
printf 'gc-postgres-password' > "$secret_dir/postgres_password.txt"
printf 'gc-minio-user' > "$secret_dir/minio_root_user.txt"
printf 'gc-minio-password-0123456789' > "$secret_dir/minio_root_password.txt"
chmod 600 "$secret_dir"/*.txt

read -r http_port minio_port console_port < <(python3 - <<'PY'
import socket
ports = []
for _ in range(3):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        ports.append(str(sock.getsockname()[1]))
print(" ".join(ports))
PY
)
env_file="$tmp/compose.env"
cat > "$env_file" <<EOF
MH_SAVE_SYNC_SECRETS_DIR="$secret_dir"
MH_SAVE_SYNC_HTTP_PORT="127.0.0.1:$http_port"
MH_SAVE_SYNC_MINIO_API_PORT="127.0.0.1:$minio_port"
MH_SAVE_SYNC_MINIO_CONSOLE_PORT="127.0.0.1:$console_port"
EOF
chmod 600 "$env_file"

compose() {
  "$runtime" compose --project-name "$project" --env-file "$env_file" \
    -f deploy/compose/compose.yaml "$@"
}
cleanup() {
  compose down -v >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT

compose up -d --build --wait
account="1111111111111111111111111111111111111111"
device="22222222222222222222222222222222"
prefix="accounts/$account/chunks"

compose exec -T minio sh -c '
  set -eu
  user="$(cat /run/secrets/minio_root_user)"
  password="$(cat /run/secrets/minio_root_password)"
  mc alias set fixture http://127.0.0.1:9000 "$user" "$password" >/dev/null
  rm -rf /tmp/gc-fixture
  mkdir -p /tmp/gc-fixture
  i=0
  while [ "$i" -lt 1005 ]; do
    printf x > "/tmp/gc-fixture/page-$i"
    i=$((i + 1))
  done
  printf tracked > /tmp/gc-fixture/tracked-orphan
  printf retained > /tmp/gc-fixture/referenced
  mc mirror --overwrite /tmp/gc-fixture "fixture/mh-save-sync/accounts/'"$account"'/chunks" >/dev/null
'

compose exec -T postgres psql -v ON_ERROR_STOP=1 -U mh_save_sync -d mh_save_sync <<SQL
INSERT INTO accounts(account_handle,root_public_key) VALUES (decode('$account','hex'),decode(repeat('33',32),'hex'));
INSERT INTO devices(cert_id,account_handle,device_public_key,certificate)
VALUES (decode('$device','hex'),decode('$account','hex'),decode(repeat('44',32),'hex'),decode(repeat('55',32),'hex'));
INSERT INTO logical_saves(id,account_handle,encrypted_label)
VALUES ('gc-save',decode('$account','hex'),'');
INSERT INTO objects(account_handle,object_id,object_kind,storage_key,size_bytes,checksum_sha256,created_at)
VALUES
  (decode('$account','hex'),'tracked-orphan','chunk','$prefix/tracked-orphan',7,'00',now()-interval '2 days'),
  (decode('$account','hex'),'referenced','chunk','$prefix/referenced',8,'00',now()-interval '2 days');
INSERT INTO snapshots(id,account_handle,logical_save_id,encrypted_manifest_object,committing_device_cert_id)
VALUES ('gc-snapshot',decode('$account','hex'),'gc-save','referenced',decode('$device','hex'));
INSERT INTO snapshot_objects(account_handle,snapshot_id,object_id)
VALUES (decode('$account','hex'),'gc-snapshot','referenced');
UPDATE logical_saves SET head_snapshot_id='gc-snapshot'
WHERE account_handle=decode('$account','hex') AND id='gc-save';
SQL

sleep 2
versions_before="$(compose exec -T minio sh -c '
  user="$(cat /run/secrets/minio_root_user)"
  password="$(cat /run/secrets/minio_root_password)"
  mc alias set fixture http://127.0.0.1:9000 "$user" "$password" >/dev/null
  mc find --versions "fixture/mh-save-sync/accounts/'"$account"'" | wc -l | tr -d " "
')"
[[ "$versions_before" -ge 1007 ]]
common_env=(
  CONTAINER_RUNTIME="$runtime"
  COMPOSE_PROJECT_NAME="$project"
  COMPOSE_ENV_FILE="$env_file"
)
dry_json="$(env "${common_env[@]}" deploy/compose/scripts/gc-orphans.sh --grace-seconds 1)"
python3 - "$dry_json" <<'PY'
import json, sys
value = json.loads(sys.argv[1])
assert value == {"eligible": 1006, "deleted": 0, "dry_run": True, "physical_purge_pending": 0, "physical_purged": 0}, value
PY

delete_json="$(env "${common_env[@]}" deploy/compose/scripts/gc-orphans.sh --grace-seconds 1 --delete)"
python3 - "$delete_json" <<'PY'
import json, sys
value = json.loads(sys.argv[1])
assert value == {"eligible": 1006, "deleted": 1006, "dry_run": False, "physical_purge_pending": 0, "physical_purged": 1006}, value
PY

db_state="$(compose exec -T postgres psql -U mh_save_sync -d mh_save_sync -Atc \
  "SELECT (SELECT count(*) FROM objects),(SELECT count(*) FROM orphan_gc_marks);")"
[[ "$db_state" == "1|0" ]]
remaining="$(compose exec -T minio sh -c '
  user="$(cat /run/secrets/minio_root_user)"
  password="$(cat /run/secrets/minio_root_password)"
  mc alias set fixture http://127.0.0.1:9000 "$user" "$password" >/dev/null
  mc find "fixture/mh-save-sync/accounts/'"$account"'" | wc -l | tr -d " "
')"
[[ "$remaining" == "1" ]]
versions_after="$(compose exec -T minio sh -c '
  user="$(cat /run/secrets/minio_root_user)"
  password="$(cat /run/secrets/minio_root_password)"
  mc alias set fixture http://127.0.0.1:9000 "$user" "$password" >/dev/null
  mc find --versions "fixture/mh-save-sync/accounts/'"$account"'" | wc -l | tr -d " "
')"
[[ "$versions_after" == "1" ]]

leak_marker="page-1004"
compose stop minio >/dev/null
set +e
error_output="$(env "${common_env[@]}" deploy/compose/scripts/gc-orphans.sh --grace-seconds 1 2>&1)"
error_status=$?
set -e
[[ $error_status -ne 0 ]]
[[ "$error_output" == *"object-store unavailable"* ]]
[[ "$error_output" != *"$leak_marker"* ]]
[[ "$error_output" != *"$account"* ]]

echo "orphan GC Compose E2E: PASS (paginated scan + physical version purge + redaction)"
