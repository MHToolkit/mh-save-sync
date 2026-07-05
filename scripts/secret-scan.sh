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

if git ls-files --others --exclude-standard -z | xargs -0 -r grep -nI -E "$pattern"; then
  echo "potential secret pattern found in untracked files" >&2
  exit 1
fi

if git ls-files | grep -Ei '(^|/)(prod|title)\.keys$|\.(3ds|cia|cci|rom|sav|mhsavebundle)$'; then
  echo "forbidden ROM/key/save/export artifact tracked" >&2
  exit 1
fi
