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
# checksums before this script runs. GitHub PR safety is enforced by the HEAD, worktree
# and index scans above; keep untracked-file scanning for local developer use only.
if [[ "${GITHUB_ACTIONS:-false}" != "true" ]]; then
  untracked_file_list="$(mktemp)"
  trap 'rm -f "$untracked_file_list"' EXIT
  git ls-files --others --exclude-standard -z > "$untracked_file_list"
  if [[ -s "$untracked_file_list" ]] && xargs -0 grep -nI -E "$pattern" < "$untracked_file_list"; then
    echo "potential secret pattern found in untracked files" >&2
    exit 1
  fi
else
  echo "Skipping untracked-file secret scan under GitHub Actions; generated CI artifacts are not source inputs."
fi

if git ls-files | grep -Ei '(^|/)(prod|title)\.keys$|\.(3ds|cia|cci|rom|sav|mhsavebundle)$'; then
  echo "forbidden ROM/key/save/export artifact tracked" >&2
  exit 1
fi
