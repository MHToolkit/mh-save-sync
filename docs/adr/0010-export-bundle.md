# ADR 0010: Self-contained versioned `.mhsavebundle`

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

An export is a versioned container containing encrypted manifest objects,
encrypted chunks, public metadata needed for graph reconstruction and a
container checksum. It is restorable with the account recovery phrase and no
server. It excludes device private keys, access tokens and server credentials.

Encrypted export is the default. Plaintext export requires an explicit
second confirmation, a distinct filename marker and a warning in the generated
metadata. Import applies the same path, size, collision, decompression and AEAD
limits as network restore.

Export creation writes a temporary file, fsyncs content and parent directory,
then atomically renames. Import never writes directly to an emulator root; it
first imports into local CAS and follows ADR 0007.
## Phase1-alpha evidence

`save-engine` exports an encrypted `.mhsavebundle` JSON container with checksum, re-imports it without a server and restores a synthetic save fixture.
