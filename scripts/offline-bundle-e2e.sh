#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

secret_hex="1111111111111111111111111111111111111111111111111111111111111111"
source_root="tests/fixtures/generic-save"
work_dir="${TMPDIR:-/tmp}/mh-save-sync-offline-bundle-$$"
bundle="artifacts/offline-bundle/generic-save.mhsavebundle"
target="$work_dir/restored"
running_target="$work_dir/running-target"
mkdir -p "$(dirname "$bundle")" "$work_dir"
trap 'rm -rf "$work_dir"' EXIT

if [[ -x target/debug/mh-save ]]; then
  cli=(target/debug/mh-save)
else
  cli=(cargo run -p save-cli --bin mh-save --)
fi

export_json="$work_dir/export.json"
restore_json="$work_dir/restore.json"
"${cli[@]}" snapshot-export \
  --root "$source_root" \
  --bundle "$bundle" \
  --secret-hex "$secret_hex" \
  > "$export_json"

"${cli[@]}" bundle-restore \
  --bundle "$bundle" \
  --target "$target" \
  --secret-hex "$secret_hex" \
  --emulator-state stopped \
  > "$restore_json"

diff -qr "$source_root" "$target" >/dev/null

if "${cli[@]}" bundle-restore \
  --bundle "$bundle" \
  --target "$running_target" \
  --secret-hex "$secret_hex" \
  --emulator-state running \
  > "$work_dir/running.stdout" \
  2> "$work_dir/running.stderr"; then
  echo "running restore unexpectedly succeeded" >&2
  exit 1
fi
if [[ -e "$running_target" ]]; then
  echo "running restore wrote target directory" >&2
  exit 1
fi
if ! grep -q "已拒绝恢复：模拟器仍在运行，没有覆盖本地存档" "$work_dir/running.stderr"; then
  echo "running restore did not explain precondition" >&2
  cat "$work_dir/running.stderr" >&2
  exit 1
fi

python3 - <<PY
import hashlib, json
from pathlib import Path
bundle = Path("$bundle")
export_data = json.loads(Path("$export_json").read_text())
restore_data = json.loads(Path("$restore_json").read_text())
print(json.dumps({
    "offline_bundle_restore": True,
    "bundle": str(bundle),
    "bundle_sha256": hashlib.sha256(bundle.read_bytes()).hexdigest(),
    "snapshot_id": export_data["snapshot_id"],
    "restored_snapshot_id": restore_data["snapshot_id"],
    "running_restore_fail_closed": True,
}, sort_keys=True))
PY
