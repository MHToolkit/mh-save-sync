# ADR 0005: High-entropy recovery root and certified device keys

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

Create a random 256-bit account recovery secret and encode it as a 24-word
high-entropy recovery phrase. A user password is never a master key.

HKDF-SHA256 with explicit versioned domain labels derives independent
authentication, root-signing seed, wrapping and deduplication keys. Each device
generates an Ed25519 keypair and receives an account-root-signed certificate.
The service stores root/device public keys, certificate metadata and revocation
state only.

macOS Keychain and Android Keystore wrap local account/device secrets. SQLite
stores only opaque key handles and public metadata. Recovery import is an
explicit operation and never reaches server logs or telemetry.

Revocation blocks future server authorization. It cannot make a compromised
device forget secrets it already obtained. V1 supports device revocation and
new-account recovery; full account-key rotation/rewrap is a versioned follow-up.
The design reserves key epochs and per-snapshot encryption versions so a later
rotation can create new wrapped manifests without silent history loss.

## Verification

Publish deterministic, non-secret vectors for phrase decoding, HKDF labels,
device certificates, keyed IDs and AEAD. Wrong key, nonce, associated data or
tag must fail closed.
## Phase1-alpha evidence

`save-crypto` tests 24-word recovery phrase round trip, HKDF domain separation, account-scoped HMAC chunk IDs, XChaCha20-Poly1305 fail-closed behavior and Ed25519 device-certificate tamper rejection.
