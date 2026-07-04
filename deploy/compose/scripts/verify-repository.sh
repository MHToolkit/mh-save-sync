#!/usr/bin/env bash
set -euo pipefail
curl -fsS http://127.0.0.1:${MH_SAVE_SYNC_PUBLIC_PORT:-18080}/ready
