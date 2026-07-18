#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "BLOCKED: macOS accessibility evidence requires Darwin" >&2
  exit 77
fi

real_home="${HOME}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"

tmp="$(mktemp -d)"
pid=""
cleanup() {
  local status=$?
  if [[ -n "$pid" ]]; then
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
  exit "$status"
}
trap cleanup EXIT

mkdir -p "$tmp/home" "$tmp/Applications"
HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" \
  ./scripts/install-macos-app.sh >/dev/null

app="$tmp/Applications/MH Save Sync.app"
HOME="$tmp/home" "$app/Contents/MacOS/MHSaveSyncMac" \
  >"$tmp/runtime.log" 2>&1 &
pid="$!"

ax_output=""
for _ in $(seq 1 20); do
  if ax_output="$(/usr/bin/osascript - "$pid" <<'APPLESCRIPT'
on run argv
  set targetPID to (item 1 of argv) as integer
  tell application "System Events"
    set matches to every process whose unix id is targetPID
    if (count of matches) is 0 then error "MHSaveSyncMac process is not present"
    tell item 1 of matches
      set menuBarCount to count of menu bars
      if menuBarCount < 1 then error "MHSaveSyncMac has no accessibility menu bar"
      set roleDescription to role description of menu bar 1
      return "menu_bars=" & menuBarCount & ";role_description=" & roleDescription
    end tell
  end tell
end run
APPLESCRIPT
  )"; then
    if [[ "$ax_output" == *"menu_bars="* ]]; then
      break
    fi
  fi
  sleep 0.25
done

[[ "$ax_output" == *"menu_bars="* ]] || {
  echo "MHSaveSyncMac accessibility menu bar was not observable" >&2
  cat "$tmp/runtime.log" >&2 || true
  exit 1
}

python3 - "$pid" "$ax_output" <<'PY'
import json
import sys

print(json.dumps({
    "macos_accessibility_e2e": True,
    "isolated_home": True,
    "menu_bar_accessibility": sys.argv[2],
    "process_pid": int(sys.argv[1]),
    "formal_data_touched": False,
}, ensure_ascii=False, sort_keys=True))
PY
