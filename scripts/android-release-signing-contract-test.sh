#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -z "${JAVA_HOME:-}" && -d "/Applications/Android Studio.app/Contents/jbr/Contents/Home" ]]; then
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
fi

release_tasks=(
  :app:assembleRelease
  :app:packageReleaseUniversalApk
  :app:signReleaseBundle
)

for task in "${release_tasks[@]}"; do
  set +e
  output="$(
    env \
      -u MH_SAVE_SYNC_ANDROID_KEYSTORE \
      -u MH_SAVE_SYNC_ANDROID_STORE_PASSWORD \
      -u MH_SAVE_SYNC_ANDROID_KEY_ALIAS \
      -u MH_SAVE_SYNC_ANDROID_KEY_PASSWORD \
      apps/android/gradlew -p apps/android "$task" --dry-run --no-daemon 2>&1
  )"
  status=$?
  set -e
  if [[ $status -eq 0 ]] || ! grep -q 'Android release signing is not configured' <<<"$output"; then
    echo "release task escaped signing gate: $task" >&2
    exit 1
  fi
done

echo "android release signing contract: ok"
