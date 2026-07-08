#!/usr/bin/env bash
set -euo pipefail

# Build a debug-signed Android Alpha APK, verify it, copy it to the local
# handoff directory and emit a redacted evidence JSON.  This script is for
# Phase 1 manual/home-device validation artifacts; it does not publish a
# stable release and it does not read emulator save data.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out_dir="${MH_SAVE_SYNC_APK_OUT_DIR:-$HOME/Games/Backups/MHSaveSync/apk}"
apk_source="${MH_SAVE_SYNC_APK_SOURCE:-$repo_root/apps/android/app/build/outputs/apk/debug/app-debug.apk}"
adb_bin="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
run_adb_smoke="${MH_SAVE_SYNC_RUN_ADB_SMOKE:-auto}"
package_name="${MH_SAVE_SYNC_ANDROID_PACKAGE:-org.mhtoolkit.savesync}"
app_label="MH 云存档"
version_name="0.1.0-alpha"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

find_sdk_tool() {
  local explicit="$1"
  local tool_name="$2"
  if [[ -n "$explicit" ]]; then
    [[ -x "$explicit" ]] || blocked "$tool_name not executable at $explicit"
    printf '%s\n' "$explicit"
    return
  fi

  local sdk_root="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
  local found=""
  found="$(find "$sdk_root/build-tools" -maxdepth 2 -type f -name "$tool_name" 2>/dev/null | sort -V | tail -n 1 || true)"
  [[ -n "$found" && -x "$found" ]] || blocked "$tool_name not found under $sdk_root/build-tools"
  printf '%s\n' "$found"
}

if [[ -z "${JAVA_HOME:-}" && -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
fi

mkdir -p "$out_dir" artifacts/runtime

head_full="$(git rev-parse HEAD)"
head_short="$(git rev-parse --short HEAD)"
artifact="$out_dir/mh-save-sync-${head_short}-debug.apk"
evidence="$out_dir/mh-save-sync-${head_short}-debug.evidence.json"
sha_file="$artifact.sha256"

(
  cd apps/android
  ./gradlew testDebugUnitTest lintDebug assembleDebug --no-daemon
)

[[ -f "$apk_source" ]] || blocked "APK not found at $apk_source after Gradle build"
cp "$apk_source" "$artifact"

apk_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
printf '%s  %s\n' "$apk_sha256" "$artifact" > "$sha_file"

apksigner_bin="$(find_sdk_tool "${APKSIGNER:-}" apksigner)"
aapt_bin="$(find_sdk_tool "${AAPT:-}" aapt)"

signature_report="$(mktemp)"
badging_report="$(mktemp)"
secret_scan_report="$(mktemp)"
apk_smoke_report="$(mktemp)"
ui_smoke_report="$(mktemp)"
cleanup() {
  rm -f "$signature_report" "$badging_report" "$secret_scan_report" "$apk_smoke_report" "$ui_smoke_report"
}
trap cleanup EXIT

"$apksigner_bin" verify --verbose --print-certs "$artifact" > "$signature_report" 2>&1
grep -q "Verified using v2 scheme (APK Signature Scheme v2): true" "$signature_report" \
  || blocked "APK v2 signature verification failed"

"$aapt_bin" dump badging "$artifact" > "$badging_report"
grep -q "package: name='$package_name'" "$badging_report" \
  || blocked "APK package mismatch; expected $package_name"
grep -q "application-label:'$app_label'" "$badging_report" \
  || blocked "APK label mismatch; expected $app_label"
grep -q "launchable-activity: name='$package_name.MainActivity'" "$badging_report" \
  || blocked "APK launchable activity mismatch"

./scripts/secret-scan.sh > "$secret_scan_report"

adb_smoke_status="skipped"
adb_smoke_reason=""
ui_smoke_status="skipped"
ui_smoke_reason=""
device_serial=""
visible_text_sha256=""

should_run_adb=false
case "$run_adb_smoke" in
  1|true|yes)
    should_run_adb=true
    ;;
  0|false|no)
    adb_smoke_reason="disabled by MH_SAVE_SYNC_RUN_ADB_SMOKE=$run_adb_smoke"
    ui_smoke_reason="$adb_smoke_reason"
    ;;
  auto)
    if [[ -x "$adb_bin" ]]; then
      device_count="$("$adb_bin" devices | awk 'NR>1 && $2=="device" {count++} END {print count+0}')"
      if [[ "$device_count" -eq 1 ]]; then
        should_run_adb=true
      else
        adb_smoke_reason="auto mode requires exactly one online adb device; found $device_count"
        ui_smoke_reason="$adb_smoke_reason"
      fi
    else
      adb_smoke_reason="adb not found at $adb_bin"
      ui_smoke_reason="$adb_smoke_reason"
    fi
    ;;
  *)
    blocked "invalid MH_SAVE_SYNC_RUN_ADB_SMOKE=$run_adb_smoke; use auto/true/false"
    ;;
esac

if [[ "$should_run_adb" == "true" ]]; then
  ADB="$adb_bin" MH_SAVE_SYNC_APK="$artifact" ./scripts/android-apk-smoke.sh > "$apk_smoke_report"
  ADB="$adb_bin" MH_SAVE_SYNC_APK="$artifact" ./scripts/android-ui-copy-smoke.sh > "$ui_smoke_report"
  adb_smoke_status="pass"
  ui_smoke_status="pass"
  device_serial="$(python3 - "$apk_smoke_report" <<'PY'
import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("device_serial", ""))
PY
)"
  visible_text_sha256="$(python3 - "$ui_smoke_report" <<'PY'
import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("visible_text_sha256", ""))
PY
)"
fi

evidence_sha256="$(
  python3 - "$evidence" "$artifact" "$apk_sha256" "$sha_file" "$head_full" "$head_short" "$signature_report" "$badging_report" "$adb_smoke_status" "$adb_smoke_reason" "$ui_smoke_status" "$ui_smoke_reason" "$device_serial" "$visible_text_sha256" <<'PY'
from pathlib import Path
import datetime
import hashlib
import json
import re
import sys

(
    evidence,
    artifact,
    apk_sha256,
    sha_file,
    head_full,
    head_short,
    signature_report,
    badging_report,
    adb_smoke_status,
    adb_smoke_reason,
    ui_smoke_status,
    ui_smoke_reason,
    device_serial,
    visible_text_sha256,
) = sys.argv[1:15]

signature = Path(signature_report).read_text(encoding="utf-8", errors="replace")
badging = Path(badging_report).read_text(encoding="utf-8", errors="replace")
launch = re.search(r"launchable-activity: name='([^']+)'", badging)
version = re.search(r"versionName='([^']+)'", badging)

signer_certificate_sha256 = ""
signer_public_key_sha256 = ""
for line in signature.splitlines():
    if line.startswith("Signer #1 certificate SHA-256 digest:") or line.startswith("V2 Signer: certificate SHA-256 digest:"):
        signer_certificate_sha256 = line.rsplit(":", 1)[1].strip()
    if line.startswith("Signer #1 public key SHA-256 digest:") or line.startswith("V2 Signer: public key SHA-256 digest:"):
        signer_public_key_sha256 = line.rsplit(":", 1)[1].strip()
if not re.fullmatch(r"[0-9a-f]{64}", signer_certificate_sha256):
    raise SystemExit(
        "missing signer certificate SHA-256 digest in apksigner output\n"
        + "\n".join(signature.splitlines()[:40])
    )
if not re.fullmatch(r"[0-9a-f]{64}", signer_public_key_sha256):
    raise SystemExit(
        "missing signer public key SHA-256 digest in apksigner output\n"
        + "\n".join(signature.splitlines()[:40])
    )

data = {
    "android_alpha_apk_evidence": True,
    "artifact": artifact,
    "artifact_sha256": apk_sha256,
    "sha256_file": sha_file,
    "built_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "repo": "MHToolkit/mh-save-sync",
    "branch": "feat/phase1-save-sync",
    "git_head": head_full,
    "git_head_short": head_short,
    "package": "org.mhtoolkit.savesync",
    "version_name": version.group(1) if version else "unknown",
    "application_label": "MH 云存档",
    "launchable_activity": launch.group(1) if launch else "",
    "signature": {
        "scheme_v2_verified": True,
        "signer_certificate_sha256": signer_certificate_sha256,
        "signer_public_key_sha256": signer_public_key_sha256,
        "debug_build": True,
    },
    "verification": {
        "gradle_test_lint_assemble": "pass",
        "apksigner_v2": "pass",
        "aapt_badging": "pass",
        "secret_scan": "pass",
        "adb_install_launch_smoke": adb_smoke_status,
        "adb_install_launch_smoke_reason": adb_smoke_reason,
        "android_ui_copy_smoke": ui_smoke_status,
        "android_ui_copy_smoke_reason": ui_smoke_reason,
        "device_serial": device_serial,
        "visible_text_sha256": visible_text_sha256,
    },
    "privacy_boundary": "package/signature/build/smoke facts only; no save tree enumeration, no recovery phrase, no token, no plaintext save bytes",
    "support_boundary": "Debug-signed Alpha APK for manual Android validation. RuntimeVerified emulator support still requires real emulator save mutate/restore/readback evidence.",
}
Path(evidence).write_text(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(hashlib.sha256(Path(evidence).read_bytes()).hexdigest())
PY
)"

printf '%s\n' "APK: $artifact"
printf '%s\n' "APK_SHA256: $apk_sha256"
printf '%s\n' "EVIDENCE: $evidence"
printf '%s\n' "EVIDENCE_SHA256: $evidence_sha256"
