# MH3G 3DS to Cemu Save Converter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and runtime-verify a fail-closed Rust CLI that converts Japanese MH3G 3DS/Nemessix saves into Japanese MH3G HD Cemu saves without modifying the source.

**Architecture:** A dedicated `mh3g-save-convert` crate owns format recognition, the evidence-backed binary transforms, transactional installation, rollback, and a small CLI. The conversion core is pure and fixture-tested; filesystem/process checks wrap it at the boundary, and real saves remain outside Git.

**Tech Stack:** Rust 1.95/edition 2024, clap, serde/serde_json, sha2, thiserror, tempfile, deterministic synthetic fixtures, Cemu/Nemessix runtime validation on macOS.

---

## File Structure

- `Cargo.toml`: register the new workspace crate.
- `crates/mh3g-save-convert/Cargo.toml`: package, binary, dependencies, and test dependencies.
- `crates/mh3g-save-convert/src/lib.rs`: public API and shared error type.
- `crates/mh3g-save-convert/src/profile.rs`: Japanese 3DS/Cemu headers, sizes, slot validation, and inspection.
- `crates/mh3g-save-convert/src/transform_table.rs`: generated, pinned endian-swap spans and special-field offsets.
- `crates/mh3g-save-convert/src/transforms.rs`: endian, monster-discovery, and arena-record transforms.
- `crates/mh3g-save-convert/src/converter.rs`: pure 3DS-to-Cemu conversion and post-conversion validation.
- `crates/mh3g-save-convert/src/transaction.rs`: process guard, SHA-256 manifest, backup, atomic install, and rollback.
- `crates/mh3g-save-convert/src/main.rs`: `inspect`, `convert`, and `rollback` CLI.
- `crates/mh3g-save-convert/tests/cli.rs`: command behavior and fail-closed filesystem tests.
- `scripts/generate-mh3g-transform-table.py`: reproducibly import the pinned upstream offset declarations.
- `docs/adr/0013-mh3g-cross-format-conversion.md`: format, ownership, migration, and rollback decision.
- `docs/DECISIONS.md`: index ADR 0013.
- `README.md`: build and operator commands.

### Task 1: Workspace and Crate Skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/mh3g-save-convert/Cargo.toml`
- Create: `crates/mh3g-save-convert/src/lib.rs`
- Create: `crates/mh3g-save-convert/src/main.rs`

- [ ] **Step 1: Add a failing workspace check**

Run: `rtk cargo test -p mh3g-save-convert`

Expected: FAIL because package `mh3g-save-convert` does not exist.

- [ ] **Step 2: Register the crate and dependencies**

Add `"crates/mh3g-save-convert"` to `workspace.members`. Create the package manifest with:

```toml
[package]
name = "mh3g-save-convert"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
clap.workspace = true
hex.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true

[[bin]]
name = "mh3g-save-convert"
path = "src/main.rs"

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 3: Add the minimal library and binary**

`src/lib.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("unsupported or invalid save: {0}")]
    InvalidSave(String),
    #[error("unsafe install refused: {0}")]
    UnsafeInstall(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

`src/main.rs` initially prints clap help through a parser named `mh3g-save-convert`.

- [ ] **Step 4: Verify and commit**

Run: `rtk cargo test -p mh3g-save-convert && rtk git diff --check`

Expected: PASS with the empty crate test target.

Commit: `feat(converter): add MH3G converter crate`

### Task 2: Japanese Format Profiles

**Files:**
- Create: `crates/mh3g-save-convert/src/profile.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`

- [ ] **Step 1: Write RED profile tests**

Add tests for `inspect_bytes` using these contracts:

```rust
assert_eq!(THREE_DS_SIZE, 0x8A00);
assert_eq!(CEMU_SIZE, 0x8A24);
assert_eq!(PAYLOAD_SIZE, 0x89FC);
assert_eq!(inspect_bytes(&jp_3ds_fixture())?.profile, SaveProfile::JpThreeDs);
assert_eq!(inspect_bytes(&jp_cemu_fixture())?.profile, SaveProfile::JpCemu);
assert!(inspect_bytes(&vec![0; THREE_DS_SIZE]).is_err());
assert!(inspect_bytes(&vec![0; THREE_DS_SIZE - 1]).is_err());
```

Run: `rtk cargo test -p mh3g-save-convert profile -- --nocapture`

Expected: FAIL because `profile` and the constants do not exist.

- [ ] **Step 2: Implement exact Japanese headers**

Use:

```rust
pub const THREE_DS_SIZE: usize = 0x8A00;
pub const CEMU_SIZE: usize = 0x8A24;
pub const PAYLOAD_SIZE: usize = 0x89FC;
pub const JP_3DS_HEADER: [u8; 4] = [0x2B, 0, 0, 0];
pub const JP_CEMU_HEADER: [u8; 40] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0x0C, 0, 0, 0x8A, 0, 0, 0, 0, 0, 0, 0, 0, 0x2B,
];
```

Define `SaveProfile::{JpThreeDs, JpCemu}` and `Inspection { profile, size, sha256 }`. Reject every other size/header combination.

- [ ] **Step 3: Add slot-path validation**

Implement `validate_slot_path(&Path)` so only basenames `user1`, `user2`, and `user3` pass. Tests must reject directories, extensions, `user0`, and `user4`.

- [ ] **Step 4: Verify and commit**

Run: `rtk cargo test -p mh3g-save-convert profile -- --nocapture`

Expected: all profile tests pass.

Commit: `feat(converter): recognize Japanese MH3G save profiles`

### Task 3: Pinned Transform Table

**Files:**
- Create: `scripts/generate-mh3g-transform-table.py`
- Create: `crates/mh3g-save-convert/src/transform_table.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`

- [ ] **Step 1: Write RED table integrity tests**

Require the generated module to expose:

```rust
assert_eq!(SWAP_SPANS.len(), 8_509);
assert_eq!(MONSTER_DISCOVERY_OFFSETS.len(), 50);
assert_eq!(ARENA_RECORD_OFFSETS.len(), 62);
assert!(SWAP_SPANS.iter().all(|s| s.start < s.end && s.end <= PAYLOAD_SIZE));
assert!(MONSTER_DISCOVERY_OFFSETS.iter().all(|&o| o + 2 <= PAYLOAD_SIZE));
assert!(ARENA_RECORD_OFFSETS.iter().all(|&o| o + 4 <= PAYLOAD_SIZE));
```

Run: `rtk cargo test -p mh3g-save-convert transform_table -- --nocapture`

Expected: FAIL because the generated table is absent.

- [ ] **Step 2: Add the reproducible importer**

The script accepts input and output paths, accepts only the pinned SHA-256 `0753baafad37147cb4701b7315a9deb9055ff699f444d55fce537b4e1ae35deb`, then emits Rust constants. Its core must be:

```python
import hashlib, runpy, sys
from pathlib import Path

EXPECTED_SHA256 = "0753baafad37147cb4701b7315a9deb9055ff699f444d55fce537b4e1ae35deb"
source, output = map(Path, sys.argv[1:3])
if hashlib.sha256(source.read_bytes()).hexdigest() != EXPECTED_SHA256:
    raise SystemExit("unexpected 3usavetools save_indices.py hash")
values = runpy.run_path(str(source))
swaps = [tuple(map(int, span)) for span in values["saveFileSwap"]]
monster = [int(offset) for offset in values["monsterDiscoveryState"]]
arena = [int(offset) for offset in values["arenaRecord"]]
if (len(swaps), len(monster), len(arena)) != (8509, 50, 62):
    raise SystemExit("unexpected transform-table counts")
```

Render each list as `pub const` Rust array literals. The generated header must include `https://github.com/fadillzzz/3usavetools`, tag `0.3.1`, commit `d20fea5d98d5c465841c8e5626dae6709622354a`, MIT license, and source SHA-256.

Run:

```bash
rtk python3 scripts/generate-mh3g-transform-table.py \
  /tmp/3usavetools-031/converter/save_indices.py \
  crates/mh3g-save-convert/src/transform_table.rs
```

- [ ] **Step 3: Verify deterministic regeneration**

Run the generator twice and then run:

```bash
rtk git diff --exit-code -- crates/mh3g-save-convert/src/transform_table.rs
rtk cargo test -p mh3g-save-convert transform_table -- --nocapture
```

Expected: no generated diff; all count/bounds assertions pass.

- [ ] **Step 4: Commit**

Commit: `feat(converter): pin MH3U transform offsets`

### Task 4: Binary Field Transforms

**Files:**
- Create: `crates/mh3g-save-convert/src/transforms.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`

- [ ] **Step 1: Write RED endian tests**

Test a two-byte and four-byte span plus an unchanged byte:

```rust
let mut payload = vec![0; PAYLOAD_SIZE];
payload[0x20..0x24].copy_from_slice(&[1, 2, 3, 4]);
apply_endian_swaps(&mut payload)?;
assert_eq!(&payload[0x20..0x24], &[4, 3, 2, 1]);
```

Run: `rtk cargo test -p mh3g-save-convert transforms -- --nocapture`

Expected: FAIL because transform functions do not exist.

- [ ] **Step 2: Implement endian swaps with bounds checks**

For each `SWAP_SPANS` entry, call `payload[start..end].reverse()`. Return `InvalidSave` before any mutation when the payload length is not exactly `PAYLOAD_SIZE`.

- [ ] **Step 3: Write and pass monster-flag tests**

Test all 16 combinations of source bits `0x01/0x02/0x04/0x08`. The conversion function must produce `0x80/0x20/0x40/0x08`, clear the second flag byte like the reference, and touch only the 50 declared records.

- [ ] **Step 4: Write and pass arena-record tests**

Port the upstream `ConverterWiiU.convertArenaRecord` equations using explicit `u16` rotation/shift operations. Test zero, all-ones, first dropped bit, second dropped bit, and alternating-bit values.

- [ ] **Step 5: Verify and commit**

Run: `rtk cargo test -p mh3g-save-convert transforms -- --nocapture`

Expected: endian, flag, and arena tests pass.

Commit: `feat(converter): implement MH3G field transforms`

### Task 5: Pure 3DS-to-Cemu Conversion

**Files:**
- Create: `crates/mh3g-save-convert/src/converter.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`

- [ ] **Step 1: Write RED conversion tests**

Use a deterministic synthetic source and assert:

```rust
let source_before = source.clone();
let output = convert_3ds_to_cemu(&source)?;
assert_eq!(source, source_before);
assert_eq!(output.len(), CEMU_SIZE);
assert_eq!(&output[..40], &JP_CEMU_HEADER);
assert_eq!(inspect_bytes(&output)?.profile, SaveProfile::JpCemu);
```

Also reject a Western `0x2C` source, a Cemu source, and a truncated source.

- [ ] **Step 2: Implement the pure pipeline**

The function must inspect the source, copy `source[4..]`, apply transforms in this order, and prepend the Japanese target header:

```rust
apply_endian_swaps(&mut payload)?;
apply_monster_discovery(&mut payload)?;
apply_arena_records(&mut payload)?;
```

Post-validate the output through `inspect_bytes` before returning it.

- [ ] **Step 3: Prove unlisted-byte preservation**

Build a boolean mask for every swapped/special byte. Assert every unmasked target payload byte equals the corresponding source payload byte.

- [ ] **Step 4: Verify and commit**

Run: `rtk cargo test -p mh3g-save-convert converter -- --nocapture`

Expected: all pure conversion tests pass.

Commit: `feat(converter): convert Japanese 3DS saves to Cemu`

### Task 6: Transactional Install and Rollback

**Files:**
- Create: `crates/mh3g-save-convert/src/transaction.rs`
- Modify: `crates/mh3g-save-convert/src/lib.rs`

- [ ] **Step 1: Write RED transaction tests**

Use `tempfile::TempDir` to cover target absent, target present, simulated validation failure, rollback with backup, rollback to target-absent state, tampered manifest, and tampered backup.

The manifest contract is:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct InstallManifest {
    pub version: u32,
    pub source_sha256: String,
    pub installed_sha256: String,
    pub previous_sha256: Option<String>,
    pub target: PathBuf,
    pub backup: Option<PathBuf>,
    pub target_previously_existed: bool,
}
```

- [ ] **Step 2: Implement process guarding**

Define an injectable `ProcessProbe` trait. The macOS implementation calls `pgrep -x` for `Nemessix`, `nemessix`, `Azahar`, `azahar`, `Cemu`, and `cemu`. A matched process returns `UnsafeInstall`; `inspect` does not invoke this guard.

- [ ] **Step 3: Implement atomic installation**

Validate the slot path, create a same-directory backup when needed, write a same-directory temporary file with `create_new`, call `sync_all`, validate the bytes, then `rename` into place. Write and sync the JSON manifest only after target installation succeeds.

- [ ] **Step 4: Implement rollback**

Verify manifest version, target path, installed target hash, and backup hash. Restore the backup atomically when it exists; otherwise remove only the installed file whose hash matches the manifest.

- [ ] **Step 5: Verify and commit**

Run: `rtk cargo test -p mh3g-save-convert transaction -- --nocapture`

Expected: all transaction and rollback tests pass.

Commit: `feat(converter): add transactional install and rollback`

### Task 7: CLI Contract

**Files:**
- Modify: `crates/mh3g-save-convert/src/main.rs`
- Create: `crates/mh3g-save-convert/tests/cli.rs`

- [ ] **Step 1: Write RED command tests**

Cover:

```text
inspect <source>
convert <source> --output <userN> --dry-run
convert <source> --output <userN> --write
rollback --manifest <manifest.json>
```

Require `--dry-run` and `--write` to conflict. Without either flag, `convert` behaves as dry-run. JSON output must include only profile, size, hashes, output, backup, manifest, and status; it must not contain decoded player data.

- [ ] **Step 2: Implement clap parsing and dispatch**

Use `PathBuf` arguments and serialize stable JSON reports. All errors print one concise line to stderr and return exit code 1. A dry run performs conversion and validation in memory but creates no output, backup, or manifest.

- [ ] **Step 3: Verify write and rollback from the binary**

Run: `rtk cargo test -p mh3g-save-convert --test cli -- --nocapture`

Expected: inspect/dry-run/write/rollback pass; invalid paths and simulated running processes fail closed.

- [ ] **Step 4: Commit**

Commit: `feat(converter): expose inspect convert and rollback CLI`

### Task 8: ADR and Operator Documentation

**Files:**
- Create: `docs/adr/0013-mh3g-cross-format-conversion.md`
- Modify: `docs/DECISIONS.md`
- Modify: `README.md`

- [ ] **Step 1: Add ADR 0013**

Record the accepted one-way Japanese profile, `0x2B` evidence, upstream `3usavetools 0.3.1` provenance, official `CTR-N-JMUJ` evidence, local-only processing, unknown-byte preservation, no runtime support claim before Cemu proof, migration path, and rollback procedure.

- [ ] **Step 2: Index the decision and document commands**

Add ADR 0013 to `docs/DECISIONS.md`. Add ready-to-run `rtk cargo run -p mh3g-save-convert -- ...` examples to `README.md`, including explicit emulator-stop preconditions.

- [ ] **Step 3: Verify and commit**

Run:

```bash
rtk cargo test -p mh3g-save-convert
rtk git diff --check
```

Expected: PASS with no documentation formatting errors.

Commit: `docs: document Japanese MH3G conversion contract`

### Task 9: Differential and Real-Save Validation

**Files:**
- No tracked real-save files
- Temporary evidence: `/tmp/mh3g-save-converter-evidence/`

- [ ] **Step 1: Run the pinned reference converter**

Copy the current Nemessix `user2` into `/tmp`, record its SHA-256, run `3usavetools 0.3.1`, and change only reference-output byte 39 from `0x2C` to the verified Japanese `0x2B`.

- [ ] **Step 2: Run this converter against the same snapshot**

Run:

```bash
rtk cargo run -p mh3g-save-convert -- inspect "$SOURCE"
rtk cargo run -p mh3g-save-convert -- convert "$SOURCE" --output "$STAGE/user2" --write
rtk cmp "$STAGE/user2" "$STAGE/reference-jp-user2"
rtk sha256sum "$SOURCE" "$STAGE/user2"
```

Expected: byte-for-byte parity with the patched Japanese reference, source hash unchanged, output size `0x8A24`, profile `jp-cemu-mh3g`.

- [ ] **Step 3: Inspect semantic checkpoints**

Compare source and converted representations for player header, money, playtime, item box, equipment box, Moga points, guild card, awards, monster log, and arena records. Record verified/unverified status without logging player strings or save contents.

- [ ] **Step 4: Record validation without committing the save**

Add only hashes, commands, tool versions, field categories, and pass/fail results to the ADR verification section. Confirm `rtk git status --short` contains no `userN`, save dump, or extracted CCI file.

Commit: `test(converter): record Japanese save differential proof`

### Task 10: Cemu Runtime Acceptance and Rollback

**Files:**
- Real target outside Git: Cemu `usr/save/00050000/10104D00/user/80000001/user2`
- Temporary evidence outside Git

- [ ] **Step 1: Preflight both emulators**

Run `rtk pgrep -fl 'Nemessix|Azahar|Cemu'` and stop immediately if any emulator is running. Record the target-absent state or target hash before installation.

- [ ] **Step 2: Install through the production CLI**

Run the converter with `--write` against the actual Cemu `user2` path. Confirm the JSON report includes an installed hash and manifest and, only if a prior target existed, a backup.

- [ ] **Step 3: Launch Japanese MH3G HD in Cemu**

Verify the game recognizes slot 2, enters the hunter, and displays matching player identity, progress, money, item box, equipment box, Moga points, guild card, awards, monster log, and arena records. Mark fields that cannot be reached in the UI as byte-preserved but semantically unverified.

- [ ] **Step 4: Exit, parse, and rollback**

Fully exit Cemu, run `inspect` on the resulting `user2`, execute `rollback --manifest`, and verify the target returns to the exact pre-install hash or the pre-install absent state.

- [ ] **Step 5: Run final verification and commit evidence**

Run:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy -p mh3g-save-convert --all-targets -- -D warnings
rtk cargo test -p mh3g-save-convert
rtk git diff --check
rtk git status --short --branch
```

Expected: all commands pass; no real save, CCI, or player content is tracked.

Commit: `test(converter): record Cemu runtime acceptance`
