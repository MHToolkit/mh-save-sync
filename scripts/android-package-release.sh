#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

secret_file="${MH_SAVE_SYNC_ANDROID_RELEASE_ENV:-$HOME/Documents/Secrets/mh-save-sync-android-release.env}"
output_dir="${MH_SAVE_SYNC_ANDROID_RELEASE_OUT_DIR:-$HOME/Games/Backups/MHSaveSync/apk/release}"
expected_cert_sha256="faa3b4e94c753bb385b3f2961de7191e5ca9f7e124f0e4a45526b3524efd28f3"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

[[ -z "$(git status --porcelain --untracked-files=normal)" ]] \
  || blocked "release packaging requires a clean Git worktree"
[[ -f "$secret_file" ]] || blocked "release signing env file not found: $secret_file"
mode="$(stat -f '%Lp' "$secret_file")"
[[ "$mode" == "600" ]] || blocked "release signing env file must use mode 600"

set -a
# shellcheck disable=SC1090
source "$secret_file"
set +a

required=(
  MH_SAVE_SYNC_ANDROID_KEYSTORE
  MH_SAVE_SYNC_ANDROID_STORE_PASSWORD
  MH_SAVE_SYNC_ANDROID_KEY_ALIAS
  MH_SAVE_SYNC_ANDROID_KEY_PASSWORD
)
for name in "${required[@]}"; do
  [[ -n "${!name:-}" ]] || blocked "missing required release signing variable: $name"
done
[[ -f "$MH_SAVE_SYNC_ANDROID_KEYSTORE" ]] || blocked "release keystore is missing"
[[ "$(stat -f '%Lp' "$MH_SAVE_SYNC_ANDROID_KEYSTORE")" == "600" ]] \
  || blocked "release keystore must use mode 600"

if [[ -z "${JAVA_HOME:-}" && -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
fi

apps/android/gradlew -p apps/android \
  :app:testDebugUnitTest :app:lintDebug :app:assembleRelease --no-daemon

apk="$repo_root/apps/android/app/build/outputs/apk/release/app-release.apk"
[[ -f "$apk" ]] || blocked "signed release APK was not produced"

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
apksigner="$(find "$sdk_root/build-tools" -maxdepth 2 -type f -name apksigner | sort -V | tail -n 1)"
[[ -x "$apksigner" ]] || blocked "apksigner is unavailable"

signature_report="$(mktemp)"
trap 'rm -f "$signature_report"' EXIT
"$apksigner" verify --verbose --print-certs "$apk" > "$signature_report"
grep -q 'Verified using v2 scheme (APK Signature Scheme v2): true' "$signature_report" \
  || blocked "release APK is not v2 signed"
actual_cert_sha256="$(
  sed -n \
    -e 's/^Signer #1 certificate SHA-256 digest: //p' \
    -e 's/^V2 Signer: certificate SHA-256 digest: //p' \
    "$signature_report" | head -n 1
)"
[[ "$actual_cert_sha256" == "$expected_cert_sha256" ]] \
  || blocked "release signer does not match the pinned MH Save Sync release certificate"

mkdir -p "$output_dir"
head_short="$(git rev-parse --short HEAD)"
artifact="$output_dir/mh-save-sync-${head_short}-release.apk"
cp "$apk" "$artifact"
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
printf '%s  %s\n' "$artifact_sha256" "$artifact" > "$artifact.sha256"
chmod 644 "$artifact" "$artifact.sha256"

printf 'APK=%s\n' "$artifact"
printf 'APK_SHA256=%s\n' "$artifact_sha256"
printf 'SIGNER_CERT_SHA256=%s\n' "$actual_cert_sha256"
