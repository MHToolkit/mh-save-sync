#!/usr/bin/env bash
set -euo pipefail

# Print the latest local Android Alpha APK path and its companion evidence file.
# This avoids hard-coding commit-specific handoff artifact names in runbooks.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

apk_dir="${MH_SAVE_SYNC_APK_OUT_DIR:-$HOME/Games/Backups/MHSaveSync/apk}"
format="${MH_SAVE_SYNC_APK_FORMAT:-env}"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

[[ -d "$apk_dir" ]] || blocked "APK directory not found: $apk_dir; run scripts/android-package-alpha.sh first"

latest="$(
  find "$apk_dir" -maxdepth 1 -type f -name 'mh-save-sync-*-debug.apk' -print0 \
    | xargs -0 ls -t 2>/dev/null \
    | head -n 1 || true
)"
[[ -n "$latest" ]] || blocked "no mh-save-sync-*-debug.apk found in $apk_dir; run scripts/android-package-alpha.sh first"

evidence="${latest%.apk}.evidence.json"
sha_file="$latest.sha256"
[[ -f "$evidence" ]] || blocked "companion evidence JSON not found: $evidence"
[[ -f "$sha_file" ]] || blocked "companion sha256 file not found: $sha_file"

apk_sha256="$(shasum -a 256 "$latest" | awk '{print $1}')"
evidence_sha256="$(shasum -a 256 "$evidence" | awk '{print $1}')"

case "$format" in
  env)
    printf 'export MH_SAVE_SYNC_APK=%q\n' "$latest"
    printf 'export MH_SAVE_SYNC_APK_EVIDENCE=%q\n' "$evidence"
    printf 'export MH_SAVE_SYNC_APK_SHA256=%q\n' "$apk_sha256"
    printf 'export MH_SAVE_SYNC_APK_EVIDENCE_SHA256=%q\n' "$evidence_sha256"
    ;;
  json)
    python3 - "$latest" "$evidence" "$apk_sha256" "$evidence_sha256" <<'PY'
import json
import sys

apk, evidence, apk_sha256, evidence_sha256 = sys.argv[1:5]
print(json.dumps({
    "latest_android_alpha_apk": True,
    "apk": apk,
    "apk_sha256": apk_sha256,
    "evidence": evidence,
    "evidence_sha256": evidence_sha256,
}, ensure_ascii=False, sort_keys=True))
PY
    ;;
  path)
    printf '%s\n' "$latest"
    ;;
  *)
    blocked "invalid MH_SAVE_SYNC_APK_FORMAT=$format; use env, json or path"
    ;;
esac
