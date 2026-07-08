#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

title_id="${MH_SAVE_SYNC_NEMESSIX_TITLE_ID:-00048100}"
save_root="${MH_SAVE_SYNC_NEMESSIX_SAVE_ROOT:-$HOME/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/00000000000000000000000000000000/00000000000000000000000000000000/title/00040000/${title_id}/data/00000001}"

if pgrep -fl "Nemessix|nemessix" >/dev/null 2>&1; then
  echo "Nemessix is running; refusing to snapshot a live save directory." >&2
  exit 20
fi

if [[ ! -d "$save_root" ]]; then
  echo "Nemessix save root not found for title ${title_id}" >&2
  exit 21
fi

case "$save_root" in
  "$HOME/Library/Application Support/Nemessix/"*) ;;
  *)
    echo "Refusing non-Nemessix path: $save_root" >&2
    exit 22
    ;;
esac

if find "$save_root" -type l -print -quit | grep -q .; then
  echo "Refusing symlink inside save root." >&2
  exit 23
fi

fingerprint() {
  python3 - "$save_root" <<'PY'
import hashlib
import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
files = []
total = 0
for path in sorted(p for p in root.rglob("*") if p.is_file()):
    rel = path.relative_to(root).as_posix()
    data = path.read_bytes()
    total += len(data)
    files.append((rel, len(data), hashlib.sha256(data).hexdigest()))

h = hashlib.sha256()
for rel, size, digest in files:
    h.update(rel.encode("utf-8"))
    h.update(b"\0")
    h.update(str(size).encode("ascii"))
    h.update(b"\0")
    h.update(digest.encode("ascii"))
    h.update(b"\n")

print(json.dumps({
    "file_count": len(files),
    "total_bytes": total,
    "tree_sha256": h.hexdigest(),
}, sort_keys=True))
PY
}

stability_seconds="${MH_SAVE_SYNC_STABILITY_SECONDS:-2}"
first="$(fingerprint)"
sleep "$stability_seconds"
second="$(fingerprint)"
if [[ "$first" != "$second" ]]; then
  echo "Nemessix save root changed during stability window; refusing snapshot." >&2
  echo "first=$first" >&2
  echo "second=$second" >&2
  exit 24
fi

cargo build -q -p save-cli --bin mh-save
snapshot_json="$(target/debug/mh-save snapshot-fixture "$save_root")"

python3 - "$title_id" "$stability_seconds" "$first" "$snapshot_json" <<'PY'
import json
import sys

title_id, stability_seconds, fp_raw, snapshot_raw = sys.argv[1:5]
fp = json.loads(fp_raw)
snapshot = json.loads(snapshot_raw)
print(json.dumps({
    "macos_nemessix_stopped_snapshot_e2e": True,
    "platform": "macOS",
    "adapter": "Nemessix 3DS",
    "title_id": title_id,
    "emulator_stopped": True,
    "stability_window_seconds": int(stability_seconds),
    "fingerprint": fp,
    "snapshot_id": snapshot["snapshot_id"],
    "snapshot_file_count": snapshot["file_count"],
    "snapshot_total_bytes": snapshot["total_bytes"],
    "manifest_entries": snapshot["manifest_entries"],
    "chunk_count": snapshot["chunk_count"],
    "support_level": "Stopped stable snapshot evidence only; not RuntimeVerified until restore is read back by the emulator after relaunch.",
}, ensure_ascii=False, sort_keys=True))
PY
