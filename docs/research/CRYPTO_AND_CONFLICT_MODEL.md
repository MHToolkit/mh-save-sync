# Cryptography, Snapshot DAG, Conflict, and Recovery Model

Status: research baseline; protocol is not frozen until the referenced ADRs and
interoperability vectors are accepted.

Last reviewed: 2026-07-04

## 1. Decision summary

MH Save Sync adopts the following composition:

- A uniformly random 256-bit account recovery secret, rendered as a
  checksum-protected 24-word recovery phrase. A password is never used directly
  as account key material.
- HKDF-SHA-256 with explicit, versioned domain separation for account identity,
  account-root signing, content-key derivation, and account-scoped deduplication.
- A random Ed25519 key pair per device. A short, deterministic-CBOR device
  certificate is signed by the account root. Snapshot commit descriptors are
  signed by the committing device.
- Fixed 1 MiB plaintext chunks in phase 1. Chunk identifiers are
  `HMAC-SHA-256(K_dedupe, version || plaintext_chunk)`, so equality is exposed
  only inside one account and not across accounts.
- Zstandard compression followed by XChaCha20-Poly1305 encryption with a fresh
  random 192-bit nonce. Paths, `GameKey`, slot details, file metadata, and
  manifests are encrypted.
- Immutable content-addressed chunks and manifests. A PostgreSQL transaction
  inserts the snapshot and performs a compare-and-swap (CAS) update of the
  logical save head only after referenced objects are durable.
- Parent snapshot IDs, not client time or file `mtime`, determine ordering.
  A stale-base commit creates a retained conflict branch; it never overwrites a
  competing branch.
- Restore always stages and verifies the selected snapshot, snapshots the
  current emulator state, acquires an emulator-stopped lock, and then performs
  atomic replacement or a journaled SAF commit with rollback.

This combines restic's immutable/CAS and crash-resistance lessons, Google and
Apple's retained conflict-version model, and Syncthing's staging/replace
discipline. It explicitly rejects silent latest-timestamp-wins.

## 2. Authoritative sources and reproducibility

Only standards, platform-owner documentation, official project documentation,
and upstream crate documentation/source are used for security decisions.
All URLs below were accessed on 2026-07-04.

| Subject | Official source | Reproduction / fact checked | Decision |
| --- | --- | --- | --- |
| HMAC | [RFC 2104](https://www.rfc-editor.org/rfc/rfc2104.html) and [RFC 4231 test vectors](https://www.rfc-editor.org/rfc/rfc4231.html) | Run the RFC 4231 SHA-256 cases against the selected `hmac` crate. | Adopt HMAC-SHA-256. |
| HKDF | [RFC 5869](https://www.rfc-editor.org/rfc/rfc5869.html) | Run Appendix A SHA-256 vectors, including empty salt/info. | Adopt extract-then-expand and versioned `info`. |
| Ed25519 | [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html) | Run section 7.1 vectors and reject malformed/non-canonical keys and signatures. | Adopt pure Ed25519 with strict verification. |
| Deterministic signed encoding | [RFC 8949 section 4.2](https://www.rfc-editor.org/rfc/rfc8949.html#name-deterministically-encoded-cbor) | Encode the same certificate/descriptor through Rust, Swift, and Kotlin fixtures and compare bytes. | Adopt deterministic CBOR; floats and indefinite-length items are forbidden. |
| ChaCha20-Poly1305 basis | [RFC 8439](https://www.rfc-editor.org/rfc/rfc8439.html) | Run section 2.8.2 AEAD vector. | Basis accepted. |
| XChaCha20-Poly1305 | [expired CFRG draft](https://datatracker.ietf.org/doc/draft-irtf-cfrg-xchacha/), [libsodium construction](https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction) | Run draft vectors and cross-decrypt RustCrypto output with libsodium. The draft is explicitly expired, so interoperability evidence is a release gate. | Adopt because the 192-bit nonce supports safe random nonce generation and deployed implementations interoperate; document the specification-status risk. |
| Zstandard | [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.html) | Decode every golden frame with an independent RFC-compatible decoder; enforce declared and actual output limits. | Adopt before encryption. |
| 24-word encoding | [BIP 39 source](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki) and [`bip39` crate API](https://docs.rs/bip39/latest/bip39/struct.Mnemonic.html) | Generate from exactly 32 random bytes, round-trip words to entropy, alter one word, and verify checksum rejection. | Adopt only the 256-bit entropy-to-24-word transport encoding and wordlist. Reject brain-wallet input and reject BIP-39 PBKDF2 wallet-seed semantics. |
| Rust AEAD implementation | [`chacha20poly1305` crate](https://docs.rs/chacha20poly1305/latest/chacha20poly1305/) | Pin the reviewed release; run upstream vectors and cross-language fixtures. Upstream reports an NCC Group audit and documents constant-time platform limits. | Candidate implementation; no reduced-round feature. |
| Rust signatures | [`ed25519-dalek`](https://docs.rs/ed25519-dalek/latest/ed25519_dalek/) | Enable `zeroize` and `rand_core`; use `verify_strict`; do not enable `legacy_compatibility` or expose `hazmat`. | Candidate implementation. |
| Rust key derivation | [`hkdf`](https://docs.rs/hkdf/latest/hkdf/) | Run RFC 5869 vectors verbatim. | Candidate implementation. |
| Secret memory handling | [`zeroize`](https://docs.rs/zeroize/latest/zeroize/) | Confirm secret owners implement `ZeroizeOnDrop`; inspect for `Copy`, debug, serialization, and reallocation. | Adopt as best-effort process-memory hygiene, not as a hardware-compromise defense. |
| macOS secret storage | [Apple Keychain guidance](https://developer.apple.com/documentation/security/storing-keys-in-the-keychain) and [`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`](https://developer.apple.com/documentation/security/ksecattraccessiblewhenunlockedthisdeviceonly) | Install, retrieve, lock, remove, and confirm the item is not synchronizable or migrated. | Wrap/store device secrets in the data-protection Keychain. |
| Android secret storage | [Android Keystore](https://developer.android.com/privacy-and-security/keystore) | Generate a non-exportable wrapping key; encrypt/decrypt a derived-key blob; test lock and app-data-clear behavior. | Use Keystore to wrap the client key blob; SQLite never stores plaintext keys. |
| restic repository lessons | [restic design](https://restic.readthedocs.io/en/v0.9.2/design.html), [interrupted-backup FAQ](https://restic.readthedocs.io/en/stable/faq.html), and [troubleshooting](https://restic.readthedocs.io/en/stable/077_troubleshooting.html) | Interrupt a synthetic upload after each persistence step; only a final snapshot/head may make data reachable. | Adopt immutable objects, locks, resumable orphan data, integrity checks, and publish-last behavior. |
| PostgreSQL concurrency | [explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html) and [`UPDATE ... RETURNING`](https://www.postgresql.org/docs/current/sql-update.html) | Race two transactions with the same expected head and assert exactly one head CAS succeeds. | Adopt a transactional conditional update, not application-side read-then-write. |
| S3 integrity | [S3 multipart overview](https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html) and [upload checksums](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html) | Interrupt multipart uploads, resume by part, submit a wrong checksum, and confirm `BadDigest`; lifecycle-abort unfinished uploads. | Adopt checksums and incomplete-upload expiry. |

### Important standards qualification

XChaCha20-Poly1305 has an expired CFRG Internet-Draft rather than a published
RFC. It is nevertheless implemented by libsodium and the audited RustCrypto
crate. This is not hidden: phase 1 requires byte-for-byte test vectors and a
libsodium cross-implementation test in CI. If this gate cannot be maintained,
the ADR must reconsider an RFC-standardized AEAD and migrate with a new
`crypto_suite` version; existing ciphertext is never silently reinterpreted.

BIP 39's own repository records implementation discouragement. MH Save Sync
therefore does **not** adopt its wallet derivation protocol or invite users to
invent a sentence. It uses the deployed English 2048-word checksum encoding
only as a human transcription of 32 CSPRNG bytes. A future replacement must be
versioned; import retains the encoding version.

## 3. Security goals and threat model

### 3.1 Protected assets

- Emulator save bytes, paths, file names, `GameKey`, region, slot, role names,
  manifest contents, exports, and account recovery secret.
- Device private keys and derived account content keys.
- Snapshot integrity, parent relationships, conflict branches, tombstones, and
  the ability to restore a previously accepted snapshot.
- Authorization to register/revoke devices and to commit or fetch objects for
  an account.
- Local original saves: a network, server, or sync failure must not make them
  unavailable or overwrite them.

### 3.2 Adversaries considered

1. A passive or active network attacker. TLS is still mandatory, while message
   signatures and AEAD provide defense in depth.
2. A curious or compromised API/database/object-store operator with complete
   PostgreSQL and S3/MinIO copies.
3. A malicious tenant attempting cross-account object access, chunk-equality
   probing, quota exhaustion, or forged graph changes.
4. A revoked, stolen, or malware-compromised device.
5. An unprivileged local process trying to read SQLite, logs, IPC, staged files,
   or exported diagnostics.
6. An unreliable or malicious server that corrupts, omits, rolls back, or
   equivocates about objects and heads.
7. Power loss, disk full, partial upload, process kill, database rollback,
   object loss, duplicate delivery, and concurrent commits.
8. A malicious manifest attempting absolute paths, `..`, symlinks, hard links,
   duplicate paths, Unicode/case collisions, oversized files, or decompression
   bombs.

### 3.3 Explicit non-goals and residual leakage

- A device compromised while keys and plaintext are in use can read that
  plaintext and may exfiltrate keys available to it.
- Device revocation prevents future server access; it cannot make a device
  forget content or keys it already obtained.
- E2EE cannot force a malicious server to retain or return data. Offline
  `.mhsavebundle` exports and independent server backups provide availability.
- The service sees an opaque account handle, opaque logical-save IDs, device
  certificate IDs, graph edges, object sizes, access timing, IP addresses,
  ciphertext counts, and quota totals. Phase 1 does not pad sizes or hide access
  patterns.
- A server can withhold the newest branch. Signed commits, remembered client
  high-water marks, and parent checks detect many rollbacks/equivocations after
  clients exchange state, but phase 1 has no public transparency witness.
- A stolen recovery phrase compromises all data encrypted under that account
  epoch. Platform key stores cannot repair an exposed paper/photographed phrase.

## 4. Account and device key lifecycle

### 4.1 Account bootstrap

1. Obtain 32 bytes `R` from the platform CSPRNG. Never accept a user-authored
   sentence or use a password as `R`.
2. Encode `R` as a version-tagged, English, 24-word checksum phrase. Display it
   once, require selected-word confirmation, prohibit clipboard by default,
   and never put it in screenshots, logs, crash reports, analytics, SQLite, or
   server requests.
3. Use HKDF-SHA-256:

   ```text
   suite_salt = SHA-256("mh-save-sync/account-root/v1")
   PRK        = HKDF-Extract(suite_salt, R)
   K_auth     = HKDF-Expand(PRK, "mh-save-sync/auth/v1", 32)
   K_sign     = HKDF-Expand(PRK, "mh-save-sync/root-signing-seed/v1", 32)
   K_wrap     = HKDF-Expand(PRK, "mh-save-sync/content-wrapping/v1", 32)
   K_dedupe   = HKDF-Expand(PRK, "mh-save-sync/dedupe/v1", 32)
   ```

4. Construct the Ed25519 account-root signing key from `K_sign`. Derive the
   public account root and an opaque account handle:

   ```text
   account_handle =
     HMAC-SHA-256(K_auth, "mh-save-sync/account-handle/v1")[0..20]
   ```

   The handle is a locator, not an authentication password. Knowledge of it
   grants no access.
5. Generate a separate random Ed25519 device signing key. Do not derive device
   keys from `R`; independent keys are necessary for meaningful revocation.
6. Sign the deterministic-CBOR device certificate with the transient account
   root. Store only the root/device public keys, certificate, status, and
   minimum graph/account metadata on the server.
7. Erase `R`, `PRK`, and `K_sign` from process memory after onboarding. Keep
   the device private key and the minimum account content-key blob
   (`K_auth`, `K_wrap`, `K_dedupe`, suite and epoch) protected by Keychain or a
   Keystore-generated non-exportable wrapping key.

The phrase is the only root recovery artifact. A normal enrolled device does
not retain the account-root signing seed, limiting a later device compromise
from minting arbitrary replacement device certificates.

### 4.2 Device certificate

The version-1 certificate is deterministic CBOR with no floating-point values:

```text
{
  cert_version: 1,
  account_root_public: bytes(32),
  cert_id: bytes(16 random),
  device_public: bytes(32),
  issued_at_server_epoch_seconds: uint,
  expires_at_server_epoch_seconds: uint,
  capabilities: uint_bitmap,
  crypto_suites: [uint],
  signature: bytes(64)
}
```

The signature covers the deterministic encoding of every field except
`signature`, prefixed with `mh-save-sync/device-certificate/v1\0`. The server:

- strictly decodes one expected representation;
- verifies the account root already associated with the account handle;
- verifies the root signature and certificate lifetime;
- rejects duplicate `cert_id`, weak/malformed public keys, unknown required
  capabilities, a revoked certificate, and downgrade-only suites;
- authenticates each request with a nonce-bound, method/path/body-hash/device
  signature and short server challenge expiry.

TLS bearer access tokens may cache successful device authentication for a short
period, but token theft is bounded and tokens are never the account root.

### 4.3 Recovery import

A new device imports the 24 words, validates checksum and encoding version,
re-derives the account root and content branches, verifies that the server's
root public key matches, creates a new random device key/certificate, and then
erases the phrase/root signing material. Recovery never accepts a server-supplied
replacement root without explicit account-reset UX.

Rate limiting protects server resources but is not treated as phrase security:
the phrase has 256 bits of computer-generated entropy.

### 4.4 Revocation and rotation boundary

- Revocation is a signed server state transition. After it commits, every API
  request and upload commit rechecks device status; cached access tokens for
  that certificate are invalidated.
- Revocation cannot revoke offline copies, previously downloaded chunks, or
  content keys already held by the device.
- Merely incrementing an epoch while deriving from the same `R` does not exclude
  a device that knows account content keys. It is key labeling, not rotation.
- Phase 1 supports device revocation but does not claim full account-secret
  rotation. The forward design creates a fresh random `R2`, a new root public
  key and content branches, records an explicitly confirmed signed transition,
  revokes all old certificates, and re-encrypts manifests/chunks into epoch 2.
  Rewrapping metadata alone is insufficient when an old device knows the
  underlying content keys.
- Old epoch data remains decryptable by an attacker who copied old keys. New
  epoch protection is forward-only. Full migration requires an inventory,
  verified re-encryption, new snapshot/head publication, and delayed old-epoch
  GC after all retained snapshots and offline export requirements are resolved.

## 5. Chunk and manifest cryptography

### 5.1 Fixed chunking and identifiers

Files are split in stable relative-path byte order into fixed 1 MiB chunks; the
last chunk may be shorter. Empty files have no data chunks but remain manifest
entries. Fixed chunking is selected because phase 1 lacks a representative
save corpus proving that content-defined chunking pays for its format,
complexity, CPU, and migration cost. CDC remains a future, versioned benchmark
decision.

For a plaintext chunk `P`:

```text
chunk_id = HMAC-SHA-256(
  K_dedupe,
  "mh-save-sync/chunk-id/v1\0" || uint64_be(len(P)) || P
)
compressed = zstd_v1(P)
K_chunk = HKDF-SHA-256(
  ikm = K_wrap,
  salt = chunk_id,
  info = "mh-save-sync/chunk-aead-key/v1",
  length = 32
)
nonce = random(24)
aad = deterministic_cbor({
  object_format: 1,
  crypto_suite: 1,
  key_epoch: 1,
  account_handle,
  chunk_id,
  plaintext_length: len(P),
  compression: "zstd",
  chunking: "fixed-1048576"
})
ciphertext = XChaCha20-Poly1305(K_chunk, nonce, compressed, aad)
```

The object is immutable at `(account_handle, key_epoch, chunk_id)`. A second
writer may reuse the existing object only after downloading and successfully
authenticating it. Random nonces are safe with the 192-bit nonce construction,
but every encryption still uses the OS CSPRNG and nonce-reuse tests.

The HMAC input uses plaintext rather than compressed bytes so a compressor
update can reduce dedupe but cannot change chunk identity. Compression and
format versions in AAD prevent reinterpretation. The server can observe
same-account equality and size; it cannot test equality across accounts without
the account-specific dedupe key.

### 5.2 Encrypted manifest

The plaintext manifest includes:

- format/schema/crypto/chunking versions and key epoch;
- full `GameKey`, adapter evidence fingerprint, slot and region;
- normalized relative path bytes, kind, logical permissions where needed,
  plaintext size, ordered chunk IDs, and whole-file keyed digest;
- tombstones, parents, device display metadata, client time plus uncertainty,
  consistency-validator evidence, and restore preconditions;
- explicit limits used when the snapshot was created.

It never contains an absolute host path. The server receives only an opaque
logical-save ID and encrypted manifest.

```text
manifest_id = random(32)
K_manifest = HKDF-SHA-256(
  ikm = K_wrap,
  salt = manifest_id,
  info = "mh-save-sync/manifest-aead-key/v1",
  length = 32
)
nonce = random(24)
aad = deterministic_cbor({
  object_format: 1,
  crypto_suite: 1,
  key_epoch: 1,
  account_handle,
  logical_save_id,
  snapshot_id,
  manifest_id
})
ciphertext = XChaCha20-Poly1305(K_manifest, nonce, zstd(manifest), aad)
```

The unencrypted commit descriptor contains only the minimum service graph:
version/suite/epoch, opaque account/logical-save/snapshot/manifest IDs, parent
snapshot IDs, committing certificate ID, encrypted object digest and size, and
an idempotency key. The device signature covers its deterministic-CBOR encoding
with the prefix `mh-save-sync/snapshot-commit/v1\0`.

### 5.3 Fail-closed rules

- Authentication failure, unknown suite/version, nonce/key length mismatch,
  manifest digest mismatch, missing chunk, wrong chunk HMAC, decompression
  overflow, or device-signature failure makes the snapshot non-restorable.
- AEAD plaintext is not exposed before tag verification. Decompression occurs
  only after authentication and under per-chunk output/time limits.
- No plaintext secret type implements unrestricted `Debug`, `Display`,
  serialization, or `Copy`. Secret buffers are best-effort zeroized.
- Logs use opaque operation/error IDs. They never include phrase words, tokens,
  keys, nonces paired with keys, plaintext paths, save bytes, or decrypted
  manifest values.

## 6. Snapshot DAG and conflict semantics

### 6.1 Core records

Each snapshot has an immutable ID, one or more parent snapshot IDs, a signed
descriptor, encrypted manifest reference, and object reachability set. The
first snapshot has no parent. A normal update has exactly one parent. A
user-resolved conflict has all resolved branch tips as parents.

The server maintains a set of active branch heads per opaque logical save, not
one mutable timestamp winner. The locally selected primary branch is UX state,
not permission to delete other heads.

### 6.2 Commit protocol

The fixed persistence order is:

1. Acquire/renew an account/logical-save upload lease. Leases bound duplicate
   work; correctness still relies on immutable objects and DB CAS.
2. `begin_snapshot(base_head, parents, signed_descriptor_draft)` returns the
   account-scoped missing chunk set and upload session.
3. Upload missing encrypted chunks; verify transport checksum and durable
   `HEAD`/metadata. Existing immutable chunks are authenticated by the client.
4. Upload the encrypted manifest; verify checksum and durable presence.
5. In one PostgreSQL transaction, recheck device status, signature, quota,
   parent existence, object confirmations, and idempotency key; insert the
   snapshot row.
6. Conditionally update the expected branch head:

   ```sql
   UPDATE logical_save_heads
      SET head_snapshot_id = :new_head,
          generation = generation + 1
    WHERE logical_save_id = :save
      AND branch_id = :branch
      AND head_snapshot_id IS NOT DISTINCT FROM :base_head
      AND generation = :expected_generation
   RETURNING generation;
   ```

7. If exactly one row returns, commit a fast-forward. If no row returns, retain
   the inserted snapshot as a new conflict branch in the same transaction.

A crash before step 5 leaves only unreachable encrypted objects/uploads.
A crash after snapshot insertion but before head CAS rolls back the transaction.
A crash after commit leaves a reachable snapshot whose objects were already
verified. No HEAD can reference a missing object by normal protocol operation.
Orphans are reclaimed only after a grace period and mark-and-sweep proves no
retained snapshot/upload/export references them.

### 6.3 Ordering and conflict rules

- If local head is an ancestor of remote head: remote fast-forward is available.
- If remote head is an ancestor of local head: upload as a fast-forward.
- If neither is an ancestor: retain both heads and enter `conflict`.
- Equal content with distinct ancestry may be auto-collapsed only by creating a
  signed merge snapshot with both parents; history is not rewritten.
- Client/server clocks and filesystem `mtime` are display evidence only. Clock
  drift cannot select or overwrite a branch.
- Delete is a signed tombstone snapshot. Delete versus modify is a conflict
  unless one is an ancestor of the other and the user explicitly advances it.
- Binary game saves are not semantically merged. Resolution options are:
  choose branch A, choose branch B, keep both under separate local profiles,
  export one/both, or cancel. Choosing creates a merge snapshot and retains
  parent history under policy/pins.
- Manual pins are never automatically removed. Default retention is latest 20,
  daily 14, weekly 8, monthly 12, with tombstones and conflict tips protected
  until resolution plus grace.

### 6.4 Rollback and equivocation detection

Every client stores the highest accepted server generation and known signed
heads for each logical save. A response below that watermark, a head without a
known ancestry proof, conflicting generations, or two different descriptors
for one snapshot ID is a hard warning and blocks automatic restore. Clients
exchange signed branch descriptors during normal sync, making server
equivocation detectable once views meet.

This does not provide global freshness while clients are isolated. A future
transparency/witness service can strengthen detection without changing
encrypted object format.

## 7. Restore and export

### 7.1 Restore transaction

1. Require the adapter's `restore_precondition`: emulator/game stopped, correct
   user root authorization, target `GameKey`/region/slot exact match, and an
   exclusive local restore lease.
2. Download into local CAS; verify descriptor signature, certificate status at
   commit, parent structure, manifest AEAD, all chunk AEAD tags, keyed IDs,
   lengths, file digests, and complete reachability.
3. Validate the entire manifest before creating destination files:
   relative paths only; no empty/absolute/`..` components; no NUL; no
   symlink/hard-link/device/FIFO/socket; no duplicate path; no Unicode or
   platform case-fold collision; bounded file count, path length, individual
   size, total size, compression ratio, CPU time, and nesting.
4. Materialize to a fresh same-filesystem staging directory using exclusive
   file creation. `fsync` files and directory metadata where supported.
5. Reconcile and snapshot the current emulator save. Restore aborts if this
   safety snapshot cannot be verified and pinned.
6. Recheck that the emulator is stopped and the target fingerprint has not
   changed since step 1.
7. On macOS/native filesystems, rename current to rollback and staging to
   current, then sync the parent directory. Do not live-overwrite files.
8. On Android SAF where directory swap is unavailable, persist a journal with
   old/new digests, copy each staged file to a temporary document, verify,
   replace one file at a time, and mark progress durably. On interruption,
   resume or roll back from the pinned pre-restore snapshot.
9. Run adapter consistency validation, record an audit event, and retain the
   rollback snapshot until explicit successful confirmation and grace expiry.

### 7.2 Offline export

`.mhsavebundle` contains the signed descriptor, encrypted manifest, encrypted
chunks, public certificates, suite/schema metadata, and an integrity inventory.
It is encrypted by default and recoverable with the account phrase without any
server. A plaintext export is a distinct command/UI action with a second
confirmation, destination warning, and no reusable account/device private key.

Export/import tests must run with the network disabled and a clean local
database. Server URLs, bearer tokens, local absolute paths, and account recovery
material are not embedded.

## 8. Server-leak and fault drills

All drills use deterministic synthetic save bytes and a disposable account.
They are acceptance plans until an evidence record links command output and
artifact hashes.

| Drill | Procedure | Required result |
| --- | --- | --- |
| Complete server disclosure | Seed unique synthetic path/role/content markers; snapshot; copy PostgreSQL and all S3 objects; search raw exports and inspect API/log output. | Markers, phrase, `GameKey`, slot, and plaintext paths are absent. Only documented metadata leakage is visible. |
| Offline decryption attempt | Give the test process DB/S3 dumps and server secrets but not the recovery phrase/client keys. Attempt manifest/chunk parse and known-plaintext guessing. | No AEAD plaintext; cross-account equality test fails. |
| Wrong key/nonce/tag | Flip each field independently and replace one ciphertext with another account's object. | Every operation fails closed before decompression or restore write. |
| Revoked device | Revoke certificate while its token and upload session are active; attempt fetch, upload, and commit. | Future operations and commit fail; already downloaded local plaintext remains, matching the explicit limitation. |
| Rollback/equivocation | Serve an older signed head, omit a known branch, and show two clients different same-generation heads. | High-water/ancestry warning; no automatic restore or overwrite. |
| Object loss | Delete a chunk and then a manifest from disposable object storage. | `check` identifies exact unreachable snapshot; HEAD is never advanced during a new incomplete commit. |
| Crash matrix | Kill after chunk PUT, manifest PUT, snapshot insert, head CAS, and response loss. Retry with same idempotency key. | Orphans are harmless; retry converges; exactly one snapshot/head result; no missing reachable object. |
| Three-device divergence | Start A/B/C at one parent, modify offline, then reconnect in shuffled order with clock offsets. | Three retained branches; order does not select a winner. |
| Delete versus modify | A commits tombstone while B modifies old parent. | Conflict retains tombstone and data branch. |
| Restore interruption | Kill during staging, current backup, native rename, and each SAF journal state. | Original or fully verified target remains; rollback snapshot can restore. |

## 9. Required automated vectors and gates

1. RFC 5869 Appendix A SHA-256 vectors.
2. RFC 4231 HMAC-SHA-256 vectors.
3. RFC 8032 section 7.1 Ed25519 vectors plus strict negative vectors.
4. RFC 8439 section 2.8.2 and the XChaCha draft AEAD vectors.
5. Cross-implementation XChaCha vectors: RustCrypto encrypt/libsodium decrypt
   and the reverse, including non-empty AAD.
6. 32-byte entropy to 24-word phrase round trips; checksum, whitespace,
   normalization, unknown word, and user-authored sentence rejection.
7. Project golden vectors for every versioned HKDF label, chunk ID, AEAD AAD,
   encrypted chunk, encrypted manifest, certificate, and signed descriptor.
   Golden fixtures contain public synthetic values only.
8. Property tests for DAG ancestry/CAS and 2-/3-device permutations; fuzz
   deterministic CBOR, manifest bounds, path collisions, corrupt frames, and
   restore journals.
9. A server-only CI job is given all database/object-store artifacts and must
   fail to decrypt; the correct client key restores exact fixture hashes.
10. Secret scanning covers logs, crash reports, test reports, Git history, CI
    artifacts, and diagnostic bundles using synthetic canary secrets.

No protocol is called stable until these vectors are version-pinned and pass on
Rust, macOS, and Android. A snapshot/crypto/schema version is never inferred
from software version.

## 10. Adopted and rejected alternatives

| Alternative | Result | Reason |
| --- | --- | --- |
| Latest `mtime`/wall-clock wins | Reject | Clock drift and concurrent offline writes silently destroy valid history. |
| Server-side plaintext or server-held recovery key | Reject | Violates E2EE and makes operator compromise a save disclosure. |
| User password as account master key | Reject | Offline guessing risk; account root is 256 CSPRNG bits. |
| Raw SHA-256 chunk IDs | Reject | Enables cross-account equality probing. |
| Global convergent encryption | Reject | Leaks equality across users and invites confirmation attacks. |
| AES-GCM for phase 1 | Not selected | Secure with rigorous unique nonces, but XChaCha's large random nonce better fits offline multi-device generation. Reconsider if XChaCha interoperability gate fails. |
| Reduced-round ChaCha variants | Reject | No need to trade security margin for unproved performance. |
| Content-defined chunking now | Defer | No representative save corpus has proved benefit; fixed 1 MiB is simpler to test and migrate. |
| Semantic binary-save merge | Reject | No safe general merge semantics across games/regions/updates. |
| Rewrap-only “rotation” | Reject as full rotation | A device that already knows content keys remains able to decrypt old data. |
| Immediate physical delete | Reject | Tombstones, retained conflicts, grace, and mark-and-sweep protect recovery. |

## 11. ADR handoff

The following decisions must be frozen in ADRs before compatibility is claimed:

- recovery phrase encoding/version and root-key lifecycle;
- exact HKDF labels, deterministic-CBOR schemas, signature prefixes, AEAD AAD,
  object envelope, and golden vectors;
- fixed-chunk manifest limits and future CDC migration;
- device certificate/request authentication and revocation cache semantics;
- server graph visibility and rollback/equivocation UX;
- branch-head schema, CAS transaction, retention/GC roots and grace;
- native and SAF restore journals;
- XChaCha specification-status acceptance and migration trigger.

