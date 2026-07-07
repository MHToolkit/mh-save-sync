#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

server_url="${MH_SAVE_SYNC_SERVER_URL:-}"
if [[ -z "$server_url" ]]; then
  blocked "set MH_SAVE_SYNC_SERVER_URL to a running mh-save-sync API, for example the isolated Alpha API"
fi
server_url="${server_url%/}"

adb="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
[[ -x "$adb" ]] || blocked "adb not found at $adb"

device_count="$("$adb" devices | awk 'NR>1 && $2=="device" {count++} END {print count+0}')"
[[ "$device_count" -eq 1 ]] || blocked "expected exactly one online adb device, found $device_count"
device_serial="$("$adb" devices | awk 'NR>1 && $2=="device" {print $1; exit}')"

tmp="$(mktemp -d)"
android_root="/sdcard/MHSaveSyncE2E"
cleanup() {
  status=$?
  if [[ "${MH_SAVE_SYNC_KEEP_ANDROID_E2E:-0}" != "1" ]]; then
    "$adb" shell rm -rf "$android_root" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
  exit "$status"
}
trap cleanup EXIT

ready_json="$tmp/ready.json"
curl -fsS "$server_url/ready" > "$ready_json"

device_json="$tmp/device-identity.json"
cargo run -q -p save-cli --bin mh-save -- crypto-device-fixture > "$device_json"

python3 - "$server_url" "$device_json" <<'PY'
import json
import sys
import urllib.error
import urllib.request

server_url, device_path = sys.argv[1:3]
identity = json.load(open(device_path, encoding="utf-8"))

def post(path, payload, expected):
    data = json.dumps(payload, separators=(",", ":")).encode()
    request = urllib.request.Request(
        server_url + path,
        data=data,
        headers={"content-type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = response.read()
            status = response.status
    except urllib.error.HTTPError as error:
        body = error.read()
        status = error.code
    if status not in expected:
        raise SystemExit(
            f"POST {path}: expected {expected}, got {status}: "
            f"{body.decode(errors='replace')}"
        )

post(
    "/v1/accounts/bootstrap",
    {
        "account_handle": identity["account_handle"],
        "root_public_key_b64": identity["root_public_key_b64"],
    },
    {200, 201, 204, 409},
)
post(
    "/v1/devices/register",
    {
        "account_handle": identity["account_handle"],
        "cert_id": identity["cert_id"],
        "device_public_key_b64": identity["device_public_key_b64"],
        "certificate_b64": identity["certificate_b64"],
    },
    {200, 201, 204, 409},
)
PY

account_handle="$(python3 - "$device_json" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["account_handle"])
PY
)"
device_cert_id="$(python3 - "$device_json" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["cert_id"])
PY
)"
run_id="$(python3 - <<'PY'
import time
print(time.time_ns())
PY
)"
logical_save_id="adb-generic-folder-${run_id}"
secret_hex="6767676767676767676767676767676767676767676767676767676767676767"

mac_dir="$tmp/macos-generic"
android_pull="$tmp/android-pulled"
restore_dir="$tmp/restored-head"
mkdir -p "$mac_dir/slot1" "$android_pull"
printf 'macos-generic-folder-head-v1\n' > "$mac_dir/slot1/main.bin"

"$adb" shell rm -rf "$android_root"
"$adb" shell mkdir -p "$android_root/source/slot1" "$android_root/restored-head"
"$adb" shell "printf 'android-generic-folder-divergent-branch\n' > '$android_root/source/slot1/main.bin'"
"$adb" pull "$android_root/source/." "$android_pull" >/dev/null

cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$mac_dir" \
  --secret-hex "$secret_hex" \
  --device-id macos-generic-folder \
  --logical-save-id "$logical_save_id" \
  --account-handle "$account_handle" \
  --device-cert-id "$device_cert_id" \
  > "$tmp/macos-upload.json"

cargo run -q -p save-cli --bin mh-save -- server-upload \
  --server-url "$server_url" \
  --root "$android_pull" \
  --secret-hex "$secret_hex" \
  --device-id "android-generic-folder-${device_serial}" \
  --logical-save-id "$logical_save_id" \
  --account-handle "$account_handle" \
  --device-cert-id "$device_cert_id" \
  > "$tmp/android-conflict.json"

cargo run -q -p save-cli --bin mh-save -- server-status \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  > "$tmp/status.json"

cargo run -q -p save-cli --bin mh-save -- server-restore \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  --target "$restore_dir" \
  --emulator-state stopped \
  > "$tmp/restore.json"

if cargo run -q -p save-cli --bin mh-save -- server-restore \
  --server-url "$server_url" \
  --secret-hex "$secret_hex" \
  --logical-save-id "$logical_save_id" \
  --target "$tmp/blocked-running" \
  --emulator-state running \
  > "$tmp/blocked.json" 2> "$tmp/blocked.err"; then
  echo "running Android generic-folder restore unexpectedly succeeded" >&2
  exit 1
fi
grep -q "restore refused while emulator is running" "$tmp/blocked.err"
if [[ -e "$tmp/blocked-running" ]]; then
  echo "running restore created target directory" >&2
  exit 1
fi

"$adb" shell rm -rf "$android_root/restored-head"
"$adb" shell mkdir -p "$android_root/restored-head"
"$adb" push "$restore_dir/." "$android_root/restored-head" >/dev/null
"$adb" pull "$android_root/restored-head/slot1/main.bin" "$tmp/restored-from-android.bin" >/dev/null
cmp "$mac_dir/slot1/main.bin" "$tmp/restored-from-android.bin"

python3 - \
  "$ready_json" \
  "$tmp/macos-upload.json" \
  "$tmp/android-conflict.json" \
  "$tmp/status.json" \
  "$tmp/restore.json" \
  "$mac_dir/slot1/main.bin" \
  "$tmp/restored-from-android.bin" \
  "$device_serial" \
  "$android_root/restored-head/slot1/main.bin" <<'PY'
import hashlib
import json
import pathlib
import sys

(
    ready_path,
    mac_path,
    android_path,
    status_path,
    restore_path,
    expected_path,
    restored_path,
    device_serial,
    android_restored_path,
) = sys.argv[1:10]
ready = json.load(open(ready_path, encoding="utf-8"))
mac = json.load(open(mac_path, encoding="utf-8"))
android = json.load(open(android_path, encoding="utf-8"))
status = json.load(open(status_path, encoding="utf-8"))
restore = json.load(open(restore_path, encoding="utf-8"))
expected = pathlib.Path(expected_path).read_bytes()
restored = pathlib.Path(restored_path).read_bytes()

assert mac["outcome"] == "first-snapshot", mac
assert mac["cloud_head"] == mac["snapshot_id"], mac
assert android["outcome"] == "conflict", android
assert android["cloud_head"] == mac["snapshot_id"], android
assert android["conflict_snapshot"] == android["snapshot_id"], android
assert status["cloud_head"] == mac["snapshot_id"], status
assert status["history_count"] == 2, status
assert status["conflict_count"] == 1, status
assert restore["snapshot_id"] == mac["snapshot_id"], restore
assert expected == restored

print(json.dumps({
    "android_generic_folder_e2e": True,
    "server_url": status["server_url"],
    "backend": ready.get("backend"),
    "adb_device": device_serial,
    "logical_save_id": status["logical_save_id"],
    "cloud_head": status["cloud_head"],
    "android_conflict_snapshot": android["snapshot_id"],
    "history_count": status["history_count"],
    "conflict_count": status["conflict_count"],
    "restored_snapshot_id": restore["snapshot_id"],
    "restored_android_path": android_restored_path,
    "restored_sha256": hashlib.sha256(restored).hexdigest(),
    "running_restore_fail_closed": True,
    "support_level": "Generic Folder Android shared-storage evidence only; does not upgrade emulator-specific adapters to RuntimeVerified",
}, ensure_ascii=False, sort_keys=True))
PY
