#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose_file="${COMPOSE_FILE:-$script_dir/../compose.yaml}"
env_file="${COMPOSE_ENV_FILE:-$HOME/Documents/Secrets/mh-save-sync.env}"
runtime="${CONTAINER_RUNTIME:-docker}"
compose_project="${COMPOSE_PROJECT_NAME:-${MH_SAVE_SYNC_COMPOSE_PROJECT:-mh-save-sync}}"
grace_seconds="${MH_SAVE_SYNC_GC_GRACE_SECONDS:-604800}"
delete=false

usage() {
  echo "usage: gc-orphans.sh [--grace-seconds N] [--delete]" >&2
}

while (($#)); do
  case "$1" in
    --grace-seconds)
      [[ $# -ge 2 ]] || { usage; exit 64; }
      grace_seconds="$2"
      shift 2
      ;;
    --delete)
      delete=true
      shift
      ;;
    *)
      usage
      exit 64
      ;;
  esac
done

if [[ ! "$grace_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "grace seconds must be a positive integer" >&2
  exit 64
fi

compose() {
  "$runtime" compose --project-name "$compose_project" --env-file "$env_file" -f "$compose_file" "$@"
}

args=(--gc-orphans --grace-seconds "$grace_seconds")
if [[ "$delete" == true ]]; then
  args+=(--delete)
fi

# The Rust process performs reachability checks and S3 deletion while holding
# PostgreSQL table locks. Shell receives only aggregate counts, never keys.
compose exec -T server /app/mh-save-server "${args[@]}"
