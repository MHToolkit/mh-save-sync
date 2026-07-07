#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
if command -v cargo-deny >/dev/null 2>&1 && command -v cargo-audit >/dev/null 2>&1; then
  ./scripts/supply-chain-gate.sh
else
  echo "cargo-deny/cargo-audit not installed; supply-chain advisory gate skipped locally" >&2
  python3 scripts/generate-sbom.py artifacts/sbom/mh-save-sync.cdx.json
fi
cargo build --workspace --bins
mkdir -p artifacts/checksums
./scripts/artifact-checksums.sh \
  target/debug/mh-save \
  target/debug/mh-save-server \
  > artifacts/checksums/rust-debug.sha256
./scripts/offline-bundle-e2e.sh
./scripts/server-sync-e2e.sh
./scripts/macos-shell-e2e.sh
./scripts/compose-server-sync-e2e-runtime-test.sh
./scripts/secret-scan.sh
