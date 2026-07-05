#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 1 ]]; then
  echo "usage: $0 <artifact> [<artifact> ...]" >&2
  exit 2
fi

for artifact in "$@"; do
  if [[ ! -f "$artifact" ]]; then
    echo "missing artifact: $artifact" >&2
    exit 1
  fi
done

shasum -a 256 "$@"
