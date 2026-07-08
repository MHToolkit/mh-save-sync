# ADR 0011: Game-specific save diff parsers

- Status: Accepted for phase1-alpha
- Date: 2026-07-08
- Owners: MHToolkit maintainers
- Review date: 2026-10-08

## Context

Conflict resolution is safer when users can see why two versions differ before
choosing local-over-cloud or cloud-over-local. Generic file hashes are useful but
not enough for confidence. However game saves are binary, proprietary and often
game/version/region specific, so a universal semantic parser would create false
confidence and could lead to lost saves.

## Decision

Add a parser contract keyed by game profile. Parsers run only on the client,
after local decryption or direct local folder reads. The server still stores
only encrypted objects and opaque graph metadata.

Phase 1 ships:

- `save-diff --game-profile mh3g-3ds --left <folder> --right <folder>`;
- `server-status.conflict_diffs`, computed locally by decrypting the current
  cloud HEAD and retained conflict branch manifests;
- `mh3g-3ds-binary-v0`, a conservative MH3G/3U 3DS parser that reports changed
  files, left/right sizes, left/right hashes and byte ranges.

`mh3g-3ds-binary-v0` does not claim semantic understanding of hunter names,
equipment, items, quest state or play time. The UI must say this clearly. Future
semantic parsers must be game-specific and evidence-backed; they cannot be reused
across 3G/3U, 4G/4U or XX/GU without explicit compatibility evidence.

## Consequences

- Users get a stronger conflict review surface immediately: "which file changed,
  how much, and where" before choosing overwrite direction.
- No service-side privacy regression: paths and save contents remain unavailable
  to the server.
- UI copy must distinguish file/byte differences from game semantics.
- Runtime Verified status still requires real emulator restore/readback evidence;
  parser fixture success is not emulator compatibility proof.

## Verification

- `cargo test -p save-engine diff -- --nocapture`
- `cargo test -p save-cli --test save_diff_cli -- --nocapture`
- `cargo test -p save-cli --test server_sync_cli -- --nocapture`
