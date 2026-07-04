# ADR 0004: Immutable encrypted manifests with fixed 1 MiB chunks

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

Snapshot format v1 contains a versioned encrypted file manifest and encrypted
chunks. The plaintext manifest contains GameKey, slot, parents, device ID,
logical time, tombstones, normalized relative paths, mode class, size,
plaintext integrity hash and ordered chunk references.

Files use fixed 1 MiB chunks. Each chunk is compressed independently with zstd,
identified by an account-scoped keyed HMAC of the compressed plaintext and
encrypted with XChaCha20-Poly1305 using a fresh random nonce. Manifests are also
authenticated ciphertext. Snapshot IDs commit to format version, encrypted
manifest object and ordered parent IDs.

The server sees opaque account-scoped object IDs, sizes and graph metadata, but
not paths, titles, character names or plaintext hashes. Chunks are durable
before the manifest; the manifest is durable before snapshot metadata/HEAD.

CDC is rejected for v1. It may be introduced only when representative save
benchmarks demonstrate a material transfer/storage benefit that offsets format
and attack-surface cost.

## Input safety

Reject absolute paths, `..`, symlinks, duplicate normalized paths, Unicode/case
collisions, unsupported special files, excess file/count/size expansion and
decompression beyond declared bounds.

## Migration and rollback

Readers dispatch on format version. Writers never rewrite existing snapshots.
Migration creates a new child snapshot; rollback selects the prior reader/writer
without mutating history.
## Phase1-alpha evidence

`save-engine` creates encrypted fixed 1 MiB chunk snapshots from a read-only staged tree, validates manifest path safety, rejects symlinks/case collisions and restores from encrypted chunks in tests.
