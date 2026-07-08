#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_exact_test() {
  local test_name="$1"
  local output
  output="$(cargo test -q -p save-client "$test_name" -- --exact)"
  printf '%s\n' "$output"
  grep -q "test result: ok. 1 passed" <<<"$output"
}

run_exact_test "tests::watcher_marks_dirty_but_never_uploads"
run_exact_test "tests::exit_and_save_complete_reconcile_dirty_session"
run_exact_test "tests::running_emulator_blocks_restore_even_when_remote_newer"

python3 - <<'PY'
import json

print(json.dumps({
    "automation_policy_e2e": True,
    "watcher_event": "dirty-only-no-upload",
    "session_boundary_events": [
        "save-complete",
        "emulator-exit",
        "periodic-reconcile",
        "manual-sync",
    ],
    "stable_snapshot_required_before_upload": True,
    "running_restore_fail_closed": True,
    "remote_download_live_overwrite": False,
}, ensure_ascii=False, sort_keys=True))
PY
