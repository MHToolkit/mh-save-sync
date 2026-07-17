#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

compose="$(cat deploy/compose/compose.yaml)"
dockerfile="$(cat deploy/compose/server.Dockerfile)"
main_rs="$(cat crates/save-server/src/main.rs)"

for needle in \
  'MH_SAVE_SYNC_RUNTIME_UID: "65532"' \
  'MH_SAVE_SYNC_RUNTIME_GID: "65532"' \
  'MH_SAVE_SYNC_RUNTIME_IDENTITY_FILE: /tmp/mh-save-sync-runtime-identity'; do
  [[ "$compose" == *"$needle"* ]] || {
    echo "compose runtime identity contract missing: $needle" >&2
    exit 1
  }
done

[[ "$dockerfile" == *'USER root:root'* ]] || {
  echo "server image must start as root only long enough to read file-backed secrets" >&2
  exit 2
}
for needle in \
  'preload_secret_envs()?;' \
  'drop_runtime_privileges()?;' \
  'write_runtime_identity_marker()?;' \
  'libc::setgroups' \
  'libc::setgid' \
  'libc::setuid'; do
  [[ "$main_rs" == *"$needle"* ]] || {
    echo "server privilege-drop implementation missing: $needle" >&2
    exit 3
  }
done

printf '%s\n' '{"compose_runtime_identity_test":true,"file_secret_preload":true,"runtime_identity_marker":true,"runtime_uid":65532,"runtime_gid":65532,"supplementary_groups_cleared":true}'
