# ADR 0017: MH3G domain-scoped compatibility repair

- Status: Proposed
- Date: 2026-08-12
- Owners: MHToolkit maintainers
- Review date: 2026-11-12

## Context

The compatibility workflow introduced by ADR 0015 has three distinct core
paths: the original 3DS slot, the continued-play Wii U/Cemu slot, and an
independent output.  Its first implementation can optionally repair received
guild cards as part of the core `repair-converted` command.  Other shared data
is handled by unrelated commands: quests are only validated and preserved,
`system` uses `convert-system`, and CEC uses an experimental import.

That split is safe at the byte level but confusing in the native interfaces.
Selecting "repair" looks like one operation while some data is repaired with
the core slot, some is not selected at all, and some uses an independent
transaction page.  Coupling cards to the core also makes one historical-version
decision and one coordinator manifest cover two different ownership domains.

The relevant data does not have one physical 3DS root.  A core `user#` and
`system` normally live in title save data, `card*` and `quest*` live in the
ExtData `user` directory, and CEC records live in a mailbox tree.  Recursively
discovering them from a broad SD-card root would be ambiguous and would violate
the explicit-path contract.

## Decision

Compatibility repair is presented as one guided workflow, but every selected
data domain is inspected, dry-run authorized, written, and rolled back
independently.

| Domain | Original input | Current authority | Output | Repair rule |
| --- | --- | --- | --- | --- |
| Core slot | 3DS `user#` | Wii U/Cemu `user#` | `user#` | Historical field-aware three-way merge |
| Guild cards | 3DS `card1`-`card3`, `cardbox` | Current matching files | Matching files | Historical field-aware three-way merge |
| Quests | 3DS `quest1`-`quest4` | Current matching files | Matching files | Preserve current bytes until a reviewed historical defect map exists |
| Shared system | 3DS `system` | Current Wii U/Cemu `system` | `system` | Union only the proven gallery/movie flag range |
| CEC | 3DS MH3G CEC mailbox | Current Wii U/Cemu `cec` | `cec` | Experimental deduplicating import; no implicit execution |

`phrase1` through `phrase3` and unknown files remain outside repair.

### Independent authorization

Each domain has its own:

1. exact source/current/output paths;
2. profile and completeness validation;
3. source/current/output-state fingerprints;
4. deterministic preview fingerprint;
5. write authorization derived from the immediately preceding Dry Run;
6. target lock, backup, manifest, and rollback action.

A successful core write does not authorize or mark cards, quests, `system`, or
CEC complete.  A failure in one domain does not silently roll back another
already completed domain.  The UI reports every selected domain separately.

The legacy `repair-converted --source-extdata-dir` option remains accepted for
existing scripts during the compatibility window.  Native UIs stop using that
coupled path and use the domain-scoped commands instead.

### File and directory selection

The native interfaces use one core selection granularity per workflow:

- directory mode resolves each independently selected root to its direct
  `user#` child;
- file mode resolves each independently selected exact `user#` file.

Changing the granularity updates the source, current, and output pickers, but
never copies one path value into another.  3DS, current Wii U/Cemu, and output
roots are independent authorities.  The CLI keeps accepting explicit mixed
file/directory resolution performed by callers for backward compatibility.

For repair, the UI never derives another domain from the core paths.  `system`
and CEC use exact file selectors.  Guild cards and quests use one shared set of
three ExtData directory selectors because both groups physically contain
direct children of the same `user` directory; nevertheless, each group binds
that directory triplet to its own Dry Run, authorization, manifest, and
rollback.  The UI shows the exact resolved files before Dry Run.  The 3DS
ExtData and CEC sources stay explicit because they are not safe to infer from
the core save parent.

### Quest boundary

The released 0.0.3-0.0.6 quest conversion only replaces the platform container
header; the quest payload is byte-compatible.  Consequently, domain-scoped
quest repair currently validates the complete group and preserves the current
Wii U bytes.  It must not overwrite continued-play quest data with the original
3DS payload.  A future write-changing quest repair requires a new reviewed
field map and synthetic historical fixtures.

### CEC boundary

CEC remains experimental.  Showing it in the same repair-scope list does not
remove its acknowledgement gate or promote file-level evidence to runtime
verification.  It keeps an independent Dry Run, manifest, and rollback.

## Consequences

- The workflow is consistent without turning unrelated files into one unsafe
  monolithic write.
- Users can repair only the affected domain and can roll it back without
  reverting later repairs in other domains.
- Every historical domain reports its detection evidence independently.  When
  the user must override an ambiguous result, the native workflow reuses one
  explicit 0.0.3-0.0.6 revision for core, guild cards, and quests because they
  came from one historical converter run.  `system` and CEC do not use that
  revision selector.
- More manifests can be produced for one user-guided repair session; the UI
  must retain and label each one.
- Complete ExtData groups remain indivisible.  Selecting one card or quest
  file does not permit a partial group write.

## Rejected alternatives

- **One recursive 3DS/MLC root scan:** ambiguous and too broad.
- **Copy the source path into the output control:** risks writing Wii U data
  beside or over 3DS data.
- **One manifest for every selected domain:** couples unrelated rollback
  lifetimes and increases partial-failure recovery complexity.
- **Blindly reconvert quests:** destroys current downloaded/created quest data.
- **Treat CEC as ordinary ExtData:** its source layout and evidence level are
  different.

## Verification and rollback

Synthetic tests must prove, for each domain:

- exact path and complete-group rejection;
- source/current/output/preview race rejection;
- current Wii U changes outside reviewed repair fields remain identical;
- missing or conflicting output fails closed unless that domain explicitly
  supports a new export target;
- write failure retains a usable recovery journal;
- rollback restores the exact previous output state;
- a second repair is idempotent.

Native UI tests must prove that changing file/directory mode changes picker
behavior without mutating another control's path, and that every selected
domain requires its own successful Dry Run before its write button is enabled.
Runtime verification remains a separate user-controlled Cemu test and is not
implied by unit, build, or synthetic file-level evidence.
