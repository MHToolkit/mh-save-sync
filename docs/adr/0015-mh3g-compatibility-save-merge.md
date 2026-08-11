# ADR 0015: MH3G compatibility save merge

- Status: Proposed for v0.0.7
- Date: 2026-07-31
- Owners: MHToolkit maintainers
- Review date: 2026-10-31

## Context

`mh3g-save-convert` versions 0.0.3 through 0.0.6 produced valid Japanese
MH3G HD Wii U/Cemu saves, but each release added narrowly scoped corrections:

| Release | Conversion behavior introduced after this release |
| --- | --- |
| 0.0.3 | Full personal/received-card arena tables and the complete numeric prefix of both Shakalaka records |
| 0.0.4 | Packed Shakalaka mask-state preservation and the shared Hunter's Notes visibility rule for received cards and offline-hall partners |
| 0.0.5 | The isolated Lamp Mask mastery `u16` at relative Shakalaka offset `0xE4` |
| 0.0.6 | Current conversion baseline |

Some players converted with an older release and then continued playing on
Wii U/Cemu. Their current Wii U slot can contain newer HR, quests, equipment,
items, guild-card statistics, companion progress, farm/fleet state, or other
mutable data that no longer exists in the original 3DS source. Re-running a
full conversion from that source would destroy valid post-conversion progress.

Older converter output does not contain a trustworthy embedded converter
version. A byte pattern can support a version hypothesis, but ordinary gameplay
can modify the same fields and erase that evidence. Therefore exact automatic
version detection cannot be guaranteed for every played save.

## Decision

Version 0.0.7 adds a separate **compatibility merge** operation. It does not
change the existing new-conversion contract.

The merge has three authorities:

1. The original 3DS data is the repair/reference input.
2. The current Wii U/Cemu data is authoritative for continued gameplay.
3. Release-specific conversion functions describe the expected old and current
   representations of the same original 3DS data.

For each bounded semantic field changed between two converter releases, the
merge performs a three-way comparison:

| Current Wii U field | Result |
| --- | --- |
| Equals the selected historical conversion | Replace with the 0.0.7 conversion |
| Equals the 0.0.7 conversion | Preserve; already repaired |
| Differs from both | Preserve as post-conversion Wii U progress and report a conflict |
| Historical and 0.0.7 outputs are identical | Preserve; the source contains no discriminator or repair |

Comparisons and writes operate on complete fields, not independent bytes. This
prevents half-updating a `u16`, `u32`, arena row, packed state, guild-card row,
or other structured value.

### Input contract

The required inputs are:

- one original Japanese 3DS `user1`, `user2`, or `user3`;
- the same-numbered current Japanese Wii U/Cemu `user#`.

The preferred current input is the complete initialized Cemu save directory.
The CLI and UIs may also accept the exact current `user#` file for a core-only
repair. Optional source/current component pairs are:

| Logical component | Original 3DS input | Current Wii U/Cemu input |
| --- | --- | --- |
| Shared system | `system` | `system` |
| Received guild cards | complete ExtData `card1`, `card2`, `card3`, `cardbox` | same four files |
| Downloaded/created quests | complete ExtData `quest1` through `quest4` | same four files |
| StreetPass/CEC | MH3G CEC mailbox | `cec` |

`phrase1` through `phrase3` and unknown files are never modified. Archives are
not opened by the Rust core. Native UIs may collect multiple files/directories,
but must resolve them into the exact component contract before invoking the
core.

All selected source components must describe the same original 3DS save set.
All current components must come from one initialized Cemu save directory.
Ambiguous duplicate basenames, incomplete selected groups, cross-slot input, or
unknown layouts are rejected.

### Release fingerprints

The classifier evaluates the bounded fields that differ across 0.0.3, 0.0.4,
0.0.5, and 0.0.6. Each candidate is generated from the original 3DS source; no
hash allowlist of real player saves is used.

The result is one of:

- `exact`: one release is supported by all available discriminators;
- `compatible-range`: multiple releases produce the same available evidence;
- `ambiguous`: discriminators conflict, usually because Wii U gameplay changed
  them;
- `unknown`: no supported historical conversion explains the input.

Classification reports per-release matched, already-current, changed, and
contradicting field counts. It never writes. A user override may select
0.0.3-0.0.6 only after the report proves that candidate is not contradicted.
An override cannot bypass profile, slot, group, or hash validation.

All selected components share one aggregate revision decision. The converter
never repairs one `user#` as one historical release and a `card*` component as
another. Conflicting component evidence produces one top-level `ambiguous`
result and requires one explicit revision selection for the complete repair.

Future converter-created outputs should receive a sidecar manifest containing
the converter version and component hashes. No unverified byte inside a game
save is reserved for converter metadata.

### Ownership matrix

| Data | Default authority | Compatibility action |
| --- | --- | --- |
| Profile, appearance, HR, money/resources, playtime | Current Wii U | Preserve |
| Inventory, equipment and storage | Current Wii U | Preserve |
| Village/port quests, urgent/event flags | Current Wii U | Preserve |
| Farm and hunting fleet | Current Wii U | Preserve |
| Personal guild card and current statistics | Current Wii U | Preserve except proven historical conversion fields |
| Arena and monster-record representation | Current Wii U values | Repair only fields still equal to the historical conversion |
| Cha-Cha/Kayamba state and mastery | Current Wii U values | Repair only historical malformed fields; preserve changed fields |
| Received cards and offline-hall partner records | Current Wii U membership/content | Repair only selected complete guild-card group fields |
| Downloaded/created quests | Current Wii U | Preserve byte-for-byte by default |
| `system` | Current Wii U | Preserve byte-for-byte by default |
| CEC cache | Current Wii U | Preserve by default; repair requires a separately proven record mapping |

The first 0.0.7 implementation supports core `user#` and complete guild-card
group repairs. Quest, system, and CEC inputs may be inspected and included in
the preview, but remain unchanged until a historical defect and a field-level
repair rule are proven.

### Transaction model

Compatibility merge is a guarded transaction:

1. Inspect and validate every selected path.
2. Classify historical conversion compatibility.
3. Produce a deterministic merge preview with component hashes, detected
   version/confidence, repaired fields, preserved changes, conflicts, and exact
   target files.
4. Dry-run returns a preview-set SHA-256 and writes nothing.
5. Write requires the original-source set SHA-256, current-target set SHA-256,
   and preview-set SHA-256 from the immediately preceding dry-run.
6. After acquiring the save-directory lock, re-read and re-hash every input.
7. Refuse writes while Cemu/Nemessix/Azahar is running.
8. Snapshot every target component before the first replacement.
9. Replace the selected components atomically as one recoverable transaction.
10. Publish a manifest only after all replacements and directory syncs pass.

If any target changes after preview, the operation fails without overwriting
it. A partial failure restores all already-replaced components. Rollback checks
the installed set hash and restores the complete pre-merge snapshot.

### CLI and UI

The implemented CLI uses separate commands:

```text
repair-converted <3DS-user#> --current <current-Cemu-user#> \
  --output <repaired-Cemu-user#> \
  [--source-extdata-dir <3DS-ExtData-user>] \
  [--from-version <0.0.3|0.0.4|0.0.5|0.0.6>] --dry-run
repair-converted <3DS-user#> --current <current-Cemu-user#> \
  --output <repaired-Cemu-user#> \
  [--source-extdata-dir <3DS-ExtData-user>] \
  [--from-version <same-selection-as-dry-run>] --write \
  --expected-source-set-sha256 ... \
  --expected-current-set-sha256 ... \
  --expected-output-set-sha256 ... \
  --expected-preview-sha256 ...
rollback-repair --manifest ...
```

Compatibility merge remains distinct from `convert`.

macOS SwiftUI and Windows WinUI expose a first-step mode selector:

- `全新转换 / New conversion`
- `修复已转换存档 / Repair a converted save`

Repair mode guides the user in order:

1. “选择当时使用的原始 3DS 存档”
2. “添加继续游玩后的当前 Wii U/Cemu 存档（只读引用）”
3. “选择修复结果的输出文件或目录”
4. “可选：添加原始 3DS ExtData 以修复已收到的公会名片”
5. inspect and show detected release/confidence;
6. show exact preserved/repaired/conflicting components;
7. dry-run;
8. guarded write and rollback location.

The native interfaces always keep current/reference and output as two visible
controls. The CLI alone retains omitted-`--output` in-place behavior for old
scripts; it is not used as a UI shortcut.

The interfaces default to system language, retain manual Chinese/English
switching, and keep feature parity. The write button remains unavailable until
the exact dry-run authorization is current.

## Rejected alternatives

- **Full reconversion over the played Wii U save:** destroys post-conversion
  progress.
- **Copy all non-fix ranges from Wii U after a new conversion:** byte ranges can
  contain mixed mutable and conversion-sensitive fields.
- **Blind byte-wise three-way merge:** can create torn structured fields.
- **Guess the old converter version from one marker:** gameplay may overwrite
  that marker.
- **Silent last-write-wins:** violates the repository restore and conflict
  contract.
- **Store a new marker inside unknown save bytes:** risks corrupting game data.

## Verification plan

Core tests must include:

- deterministic synthetic outputs for 0.0.3, 0.0.4, 0.0.5, and 0.0.6;
- exact, compatible-range, ambiguous, and unknown classification;
- simulated post-conversion mutations in profile, HR, inventory, equipment,
  quests, cards, companions, farm, fleet, arena, and monster records;
- proof that unrelated Wii U bytes remain byte-identical;
- field-atomic conflicts, including one changed byte inside a multi-byte field;
- complete guild-card group behavior and incomplete-group rejection;
- source/current/output/preview hash race rejection;
- injected failure after each replacement and complete rollback;
- idempotence: applying the same merge twice produces identical bytes;
- all existing conversion, ExtData, CEC, CLI, macOS presentation, and Windows
  source checks remain green.

Local validation may establish static/build/file-level evidence. Runtime
verification still requires a user-controlled Cemu session and must be reported
separately; the implementation must not launch Cemu automatically.

## Migration and rollback

The compatibility merge does not reuse the single-file conversion manifest.
Coordinator manifest version 2 records the output-state hash introduced by the
three-path flow; `rollback-repair` continues accepting version 1 manifests
created by the original in-place flow. Existing 0.0.3-0.0.6 manifests remain
valid for their original rollback commands.

The feature is additive. Existing conversion commands remain unchanged;
compatibility-merged saves are restored through their retained merge manifest
and snapshots.
