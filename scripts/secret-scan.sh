#!/usr/bin/env bash
set -euo pipefail
pattern="(AKIA[0-9A-Z]{16}|BEGIN .*PRIVATE KEY|recovery[_ -]?phrase[[:space:]]*[:=]|secret[_-]?access[_-]?key[[:space:]]*[:=]|(access|refresh)[_-]?token[[:space:]]*[:=][[:space:]]*[A-Za-z0-9_./+=-]{20,}|password[[:space:]]*=[[:space:]]*[A-Za-z0-9_./+=-]{20,})"
exclude=(-- ':!deploy/compose/secrets/.example' ':!scripts/secret-scan.sh' ':!deny.toml')

if git grep -n -I -E "$pattern" HEAD "${exclude[@]}"; then
  echo "potential secret pattern found in HEAD" >&2
  exit 1
fi

if git grep -n -I -E "$pattern" "${exclude[@]}"; then
  echo "potential secret pattern found in worktree" >&2
  exit 1
fi

if git grep --cached -n -I -E "$pattern" "${exclude[@]}"; then
  echo "potential secret pattern found in index" >&2
  exit 1
fi

# CI steps intentionally create untracked build outputs, generated bindings, SBOMs and
# checksums before this script runs. Scan untracked source-like files only; generated
# artifacts are covered by the tracked-source checks above and would otherwise cause
# false positives on schema/example field names such as access_token or password.
if git ls-files --others --exclude-standard -z \
  ':!:artifacts/**' \
  ':!:target/**' \
  ':!:apps/android/.gradle/**' \
  ':!:apps/android/app/build/**' \
  ':!:apps/macos/.build/**' \
  | xargs -0 -r grep -nI -E "$pattern"; then
  echo "potential secret pattern found in untracked source-like files" >&2
  exit 1
fi

if git ls-files | grep -Ei '(^|/)(prod|title)\.keys$|\.(3ds|cia|cci|rom|sav|mhsavebundle)$'; then
  echo "forbidden ROM/key/save/export artifact tracked" >&2
  exit 1
fi
