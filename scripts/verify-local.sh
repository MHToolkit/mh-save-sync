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
./scripts/automation-policy-e2e.sh
python3 scripts/ux-copy-guard.py
./scripts/macos-shell-e2e.sh
./scripts/macos-config-e2e.sh
./scripts/build-macos-app-bundle.sh
./scripts/macos-install-e2e.sh
if [[ -z "${JAVA_HOME:-}" && -x "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java" ]]; then
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
fi
if [[ -n "${JAVA_HOME:-}" && -x "${JAVA_HOME}/bin/java" ]]; then
  ./scripts/android-release-signing-contract-test.sh
  ./apps/android/gradlew -p apps/android assembleDebug testDebugUnitTest lintDebug --no-daemon
else
  echo "JAVA_HOME not set and Android Studio JBR not found; Android local gate skipped" >&2
fi
./scripts/compose-server-sync-e2e-runtime-test.sh
./scripts/compose-project-volume-test.sh
./scripts/compose-tls-config-test.sh
./scripts/compose-runtime-identity-test.sh
./scripts/secret-scan.sh
