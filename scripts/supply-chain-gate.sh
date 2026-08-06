#!/usr/bin/env bash
set -euo pipefail

tool_version_deny="${CARGO_DENY_VERSION:-0.19.9}"
tool_version_audit="${CARGO_AUDIT_VERSION:-0.22.2}"

maybe_install() {
  local bin="$1"
  local crate="$2"
  local version="$3"
  if command -v "$bin" >/dev/null 2>&1; then
    return 0
  fi
  if [[ "${MH_SAVE_SYNC_INSTALL_SUPPLY_CHAIN_TOOLS:-0}" == "1" ]]; then
    cargo install "$crate" --version "$version" --locked
    return 0
  fi
  echo "$bin not installed; set MH_SAVE_SYNC_INSTALL_SUPPLY_CHAIN_TOOLS=1 to install pinned $crate $version" >&2
  return 1
}

cargo fetch --locked

maybe_install cargo-deny cargo-deny "$tool_version_deny"
cargo deny check advisories licenses bans sources

maybe_install cargo-audit cargo-audit "$tool_version_audit"
audit_ignore_args=()
for advisory in ${CARGO_AUDIT_IGNORE_IDS:-RUSTSEC-2026-0194 RUSTSEC-2026-0195}; do
  audit_ignore_args+=(--ignore "$advisory")
done
cargo audit "${audit_ignore_args[@]}"

python3 scripts/generate-sbom.py dependencies artifacts/sbom/mh-save-sync.cdx.json
python3 scripts/generate-sbom.py verify-dependencies \
  --sbom artifacts/sbom/mh-save-sync.cdx.json
