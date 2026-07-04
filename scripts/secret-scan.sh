#!/usr/bin/env bash
set -euo pipefail
pattern="(AKIA[0-9A-Z]{16}|BEGIN .*PRIVATE KEY|recovery[_ -]?phrase[[:space:]]*[:=]|secret[_-]?access[_-]?key[[:space:]]*[:=]|(access|refresh)[_-]?token[[:space:]]*[:=][[:space:]]*[A-Za-z0-9_./+=-]{20,}|password[[:space:]]*=[[:space:]]*[A-Za-z0-9_./+=-]{20,})"
for mode in --cached --untracked; do
  if git grep "$mode" -n -I -E "$pattern" -- ':!deploy/compose/secrets/.example' ':!scripts/secret-scan.sh'; then
    echo "potential secret pattern found ($mode)" >&2
    exit 1
  fi
done
