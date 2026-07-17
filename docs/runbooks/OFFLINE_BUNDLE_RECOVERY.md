# Offline `.mhsavebundle` recovery runbook

- Status: phase1-alpha synthetic fixture runbook
- Last verified: 2026-07-07
- Scope: no-server export/import recovery using synthetic fixture data only.
- Secret policy: commands below use a fixed public fixture secret. Do not reuse
  this value for a real account recovery secret or real saves.

## Why this exists

Cloud sync must not be the only recovery path. A user must be able to export an
end-to-end encrypted `.mhsavebundle`, move it to another device, and restore it
without the PostgreSQL/S3 service being reachable. Restore still follows the
same safety invariant: emulator stopped first, then restore into staging/target;
running-emulator restore fails closed.

## One-command verification

```bash
./scripts/offline-bundle-e2e.sh
```

Expected output shape:

```json
{
  "offline_bundle_restore": true,
  "bundle": "artifacts/offline-bundle/generic-save.mhsavebundle",
  "bundle_sha256": "...",
  "snapshot_id": "...",
  "restored_snapshot_id": "...",
  "running_restore_fail_closed": true
}
```

The script performs all of the following. The snapshot and bundle hashes change on each run because encrypted manifests/chunks use fresh AEAD nonces; that is expected and preserves semantic security. The restored bytes must still match the source fixture exactly:

1. Export `tests/fixtures/generic-save` to an encrypted bundle.
2. Restore that bundle to a fresh directory with `--emulator-state stopped`.
3. Byte-compare the restored directory with the fixture source.
4. Attempt a second restore with `--emulator-state running` and require failure.
5. Require that the failed running restore does not create the target directory.

## Manual commands

```bash
secret_hex="1111111111111111111111111111111111111111111111111111111111111111"
mkdir -p artifacts/offline-bundle /tmp/mh-save-sync-restored

cargo run -p save-cli --bin mh-save -- snapshot-export \
  --root tests/fixtures/generic-save \
  --bundle artifacts/offline-bundle/generic-save.mhsavebundle \
  --secret-hex "$secret_hex"

cargo run -p save-cli --bin mh-save -- bundle-restore \
  --bundle artifacts/offline-bundle/generic-save.mhsavebundle \
  --target /tmp/mh-save-sync-restored/generic-save \
  --secret-hex "$secret_hex" \
  --emulator-state stopped

diff -qr tests/fixtures/generic-save /tmp/mh-save-sync-restored/generic-save
```

Fail-closed precondition check:

```bash
cargo run -p save-cli --bin mh-save -- bundle-restore \
  --bundle artifacts/offline-bundle/generic-save.mhsavebundle \
  --target /tmp/mh-save-sync-running-restore \
  --secret-hex "$secret_hex" \
  --emulator-state running
# expected non-zero: restore refused while emulator is running
```

## Current boundary

This is fixture-backed CLI recovery evidence. It proves the portable encrypted
bundle path and restore precondition enforcement, but it does not upgrade any
emulator adapter to `RuntimeVerified`. Runtime verification still requires a real
emulator save -> mutate/damage -> restore -> emulator-readable loop.
