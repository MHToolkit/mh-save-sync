#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

secret_file="${MH_SAVE_SYNC_ANDROID_RELEASE_ENV:-$HOME/Documents/Secrets/mh-save-sync-android-release.env}"
old_secret_file="${MH_SAVE_SYNC_ANDROID_OLD_SIGNER_ENV:-$HOME/Documents/Secrets/mh-save-sync-android-old-signer.env}"
lineage_file="${MH_SAVE_SYNC_ANDROID_LINEAGE:-$HOME/Documents/Secrets/mh-save-sync-android-signing-lineage.bin}"
old_keystore="${MH_SAVE_SYNC_ANDROID_OLD_KEYSTORE:-$HOME/.android/debug.keystore}"
old_alias="${MH_SAVE_SYNC_ANDROID_OLD_KEY_ALIAS:-androiddebugkey}"
old_store_password_env="${MH_SAVE_SYNC_ANDROID_OLD_STORE_PASSWORD_ENV:-MH_SAVE_SYNC_ANDROID_OLD_STORE_PASSWORD}"
old_key_password_env="${MH_SAVE_SYNC_ANDROID_OLD_KEY_PASSWORD_ENV:-MH_SAVE_SYNC_ANDROID_OLD_KEY_PASSWORD}"
output_dir="${MH_SAVE_SYNC_ANDROID_MIGRATION_OUT_DIR:-$HOME/Games/Backups/MHSaveSync/apk/migration}"
version_code="${MH_SAVE_SYNC_ANDROID_MIGRATION_VERSION_CODE:-4}"
version_name="${MH_SAVE_SYNC_ANDROID_MIGRATION_VERSION_NAME:-0.1.0-alpha.3-signer-migration.1}"
expected_old_cert_sha256="ef44f7a19b5029bda21cb2644b8d3ec49d17633d49e0e165b42f991cfe5adedb"
expected_new_cert_sha256="faa3b4e94c753bb385b3f2961de7191e5ca9f7e124f0e4a45526b3524efd28f3"

blocked() {
  echo "BLOCKED: $*" >&2
  exit 77
}

secret_mode_is_600() {
  [[ -f "$1" && "$(stat -f '%Lp' "$1")" == "600" ]]
}

[[ -z "$(git status --porcelain --untracked-files=normal)" ]] \
  || blocked "signer migration packaging requires a clean Git worktree"
secret_mode_is_600 "$secret_file" || blocked "release signing env must exist with mode 600"
secret_mode_is_600 "$old_secret_file" || blocked "old signer env must exist with mode 600"
secret_mode_is_600 "$lineage_file" || blocked "signing lineage must exist with mode 600"
[[ -f "$old_keystore" ]] || blocked "old signer keystore is missing"
[[ "$version_code" =~ ^[1-9][0-9]*$ ]] || blocked "migration versionCode must be a positive integer"
[[ -n "$version_name" ]] || blocked "migration versionName must not be blank"

set -a
# shellcheck disable=SC1090
source "$secret_file"
# shellcheck disable=SC1090
source "$old_secret_file"
set +a

required=(
  MH_SAVE_SYNC_ANDROID_KEYSTORE
  MH_SAVE_SYNC_ANDROID_STORE_PASSWORD
  MH_SAVE_SYNC_ANDROID_KEY_ALIAS
  MH_SAVE_SYNC_ANDROID_KEY_PASSWORD
  "$old_store_password_env"
  "$old_key_password_env"
)
for name in "${required[@]}"; do
  [[ -n "${!name:-}" ]] || blocked "missing required signing variable: $name"
done
secret_mode_is_600 "$MH_SAVE_SYNC_ANDROID_KEYSTORE" || blocked "release keystore must use mode 600"

if [[ -z "${JAVA_HOME:-}" && -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
fi

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
apksigner="$(find "$sdk_root/build-tools" -maxdepth 2 -type f -name apksigner | sort -V | tail -n 1)"
aapt="$(find "$sdk_root/build-tools" -maxdepth 2 -type f -name aapt | sort -V | tail -n 1)"
[[ -x "$apksigner" ]] || blocked "apksigner is unavailable"
[[ -x "$aapt" ]] || blocked "aapt is unavailable"

export MH_SAVE_SYNC_ANDROID_VERSION_CODE="$version_code"
export MH_SAVE_SYNC_ANDROID_VERSION_NAME="$version_name"
apps/android/gradlew -p apps/android \
  :app:testDebugUnitTest :app:lintDebug :app:assembleRelease --no-daemon

input_apk="$repo_root/apps/android/app/build/outputs/apk/release/app-release.apk"
[[ -f "$input_apk" ]] || blocked "release APK was not produced"

mkdir -p "$output_dir"
head_short="$(git rev-parse --short HEAD)"
artifact="$output_dir/mh-save-sync-${head_short}-signer-migration.apk"
lineage_report="$(mktemp)"
signature_report="$(mktemp)"
badging_report="$(mktemp)"
cleanup() {
  rm -f "$lineage_report" "$signature_report" "$badging_report"
}
trap cleanup EXIT

"$apksigner" sign \
  --out "$artifact" \
  --v1-signing-enabled false \
  --v2-signing-enabled true \
  --v3-signing-enabled true \
  --v4-signing-enabled false \
  --rotation-min-sdk-version 28 \
  --ks "$old_keystore" \
  --ks-key-alias "$old_alias" \
  --ks-pass "env:$old_store_password_env" \
  --key-pass "env:$old_key_password_env" \
  --next-signer \
  --ks "$MH_SAVE_SYNC_ANDROID_KEYSTORE" \
  --ks-key-alias "$MH_SAVE_SYNC_ANDROID_KEY_ALIAS" \
  --ks-pass env:MH_SAVE_SYNC_ANDROID_STORE_PASSWORD \
  --key-pass env:MH_SAVE_SYNC_ANDROID_KEY_PASSWORD \
  --lineage "$lineage_file" \
  "$input_apk"

"$apksigner" verify --verbose --print-certs "$artifact" > "$signature_report"
"$apksigner" lineage --in "$artifact" --print-certs > "$lineage_report"
"$aapt" dump badging "$artifact" > "$badging_report"

grep -Fqx 'Verified using v3 scheme (APK Signature Scheme v3): true' "$signature_report" \
  || blocked "migration APK is not v3 signed"
actual_current_cert_sha256="$(
  sed -n 's/^Signer #1 certificate SHA-256 digest: //p' "$signature_report" | head -n 1
)"
actual_old_cert_sha256="$(
  sed -n 's/^Signer #1 in lineage certificate SHA-256 digest: //p' "$lineage_report" | head -n 1
)"
actual_new_cert_sha256="$(
  sed -n 's/^Signer #2 in lineage certificate SHA-256 digest: //p' "$lineage_report" | head -n 1
)"
old_installed_data_capability="$(
  awk '
    /^Signer #1 in lineage certificate DN:/ { in_old = 1; next }
    /^Signer #2 in lineage certificate DN:/ { in_old = 0 }
    in_old && /^Has installed data capability:/ { print $NF; exit }
  ' "$lineage_report"
)"
[[ "$actual_current_cert_sha256" == "$expected_new_cert_sha256" ]] \
  || blocked "migration APK current signer is not the production certificate"
[[ "$actual_old_cert_sha256" == "$expected_old_cert_sha256" ]] \
  || blocked "lineage does not begin with the installed debug certificate"
[[ "$old_installed_data_capability" == "true" ]] \
  || blocked "old signer lacks installed-data migration capability"
[[ "$actual_new_cert_sha256" == "$expected_new_cert_sha256" ]] \
  || blocked "lineage does not terminate at the production certificate"
grep -q "versionCode='$version_code'" "$badging_report" \
  || blocked "migration APK versionCode mismatch"
grep -q "versionName='$version_name'" "$badging_report" \
  || blocked "migration APK versionName mismatch"

artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
printf '%s  %s\n' "$artifact_sha256" "$artifact" > "$artifact.sha256"
chmod 644 "$artifact" "$artifact.sha256"

printf 'APK=%s\n' "$artifact"
printf 'APK_SHA256=%s\n' "$artifact_sha256"
printf 'VERSION_CODE=%s\n' "$version_code"
printf 'VERSION_NAME=%s\n' "$version_name"
printf 'PREVIOUS_SIGNER_CERT_SHA256=%s\n' "$expected_old_cert_sha256"
printf 'CURRENT_SIGNER_CERT_SHA256=%s\n' "$expected_new_cert_sha256"
printf 'SIGNATURE_SCHEME_V3=true\n'
printf 'INSTALLED_DATA_CAPABILITY=true\n'
