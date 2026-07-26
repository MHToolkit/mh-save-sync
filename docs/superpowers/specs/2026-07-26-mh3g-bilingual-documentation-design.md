# MH3G Converter Bilingual Documentation Design

Date: 2026-07-26
Status: Approved direction, pending implementation-plan review

## Goal

Make the Japanese MH3G 3DS-to-Wii U/Cemu converter usable by Chinese players
without reducing its usefulness to international open-source users. Document
the exact source files, destination files, safety workflow, optional guild-card
data, and experimental StreetPass CEC path. Back the documented macOS commands
with isolated real-save CLI validation and keep Windows claims limited to the
evidence actually produced by GitHub Actions and later Win11 tester feedback.

## Documentation Structure

The repository root keeps `README.md` as the complete English entry point and
adds a prominent language switch linking to a complete Simplified Chinese
mirror, `README.zh-CN.md`. The Chinese README links back to English. This avoids
doubling every paragraph in one page while making Chinese a first-class,
one-click entry point.

The exact MH3G component contract remains in
`docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md`. A complete Chinese counterpart,
`docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md`, will mirror its tables and
read/write boundaries. Both documents will cross-link at the top.

Standalone Windows and macOS archives will use source-controlled package
README templates. Each package README will contain Chinese first and English
second because archive users may never visit GitHub. The Windows workflow will
copy its template rather than embedding prose in YAML. A macOS package template
will be available to the current manual/release packaging path. Unrelated
deployment and engineering research documents are outside this change.

## Storage Terminology and File Contract

Both languages will distinguish these three independent 3DS storage areas:

1. Title savedata under
   `sdmc/Nintendo 3DS/<ID0>/<ID1>/title/00040000/00048100/data/00000001/`,
   containing `user1`, `user2`, `user3`, and `system`.
2. MH3G shared ExtData under
   `sdmc/Nintendo 3DS/<ID0>/<ID1>/extdata/00000000/00000481/user/`,
   containing `card1`, `card2`, `card3`, `cardbox`, and `quest1` through
   `quest4`. The converter input is the `user` child, not the `00000481` root.
3. The system StreetPass CEC mailbox under
   `nand/data/<ID0>/sysdata/00010026/00000000/CEC/00048100/`. This is a NAND
   system mailbox, not MH3G title savedata or SD-card ExtData.

CEC documentation will state that `InBox___/_*` contains received raw messages,
`OutBox__/_*` contains the local hunter's outgoing broadcast, and
`BoxInfo_____` stores mailbox metadata. Only non-empty received inbox messages
are candidates for `convert-cec`; outbox records are intentionally ignored.
CEC conversion is independent, optional, and experimental. Existing durable
guild cards and offline-hall partners use the matching `user#` plus `card1`,
`card2`, `card3`, and `cardbox`; an empty CEC inbox does not imply that the card
list is empty.

The documents will explicitly identify `boss/`, `icon`, `metadata`, and
`phrase1` through `phrase3` as not read or written by the current converter.
They will describe `system`, quests, guild cards, and CEC as separate optional
groups rather than implying that a core slot conversion recursively changes an
entire save directory.

## Accepted Input Shapes

The README must make input type visible before its first conversion example.
The current CLI does not auto-discover a complete save tree and does not read
ZIP archives directly:

| Command group | Accepted input | Not accepted |
| --- | --- | --- |
| `inspect`, `inspect-progress`, `inspect-events`, `convert` | One explicit `user1`, `user2`, or `user3` file | Slot directory, ExtData directory, ZIP |
| `convert-system` | One explicit `system` file | Title savedata directory, ZIP |
| `convert-extras` | The exact ExtData `.../00000481/user` directory with all eight `card*`/`quest*` files directly inside | `00000481` parent, partial file set, ZIP |
| `inspect-cec`, `convert-cec` | The exact CEC `.../CEC/00048100` directory containing `InBox___` | SD-card ExtData directory, ZIP |
| `rollback`, `rollback-cec` | One explicit converter-generated manifest file | Save directory, backup file, ZIP |

ZIP, 7z, RAR, QQ preview, and browser archive-preview inputs must be fully
extracted to a normal local directory first. Documentation will show how to
recognize the correct extracted level by its immediate children. Paths that
contain spaces must be quoted. The converter never recursively searches an
arbitrary folder for a plausible save, because accidental file selection would
weaken its fail-closed behavior.

## Player Workflow

The quick-start path in both languages will follow the same fail-closed order:

1. Fully stop Nemessix, Azahar, and Cemu before writes.
2. Select one source `user#` and the same-numbered Cemu destination.
3. Run `inspect` and the relevant read-only inspectors.
4. Run `convert --dry-run` and retain the reported hashes.
5. Run `convert --write`, which creates transactional metadata and a backup
   when a destination already exists.
6. Validate in Cemu manually, then retain or remove artifacts deliberately.
7. Use the manifest-bound rollback command if validation fails.

Guild-card migration will separately require the complete eight-file ExtData
`user` directory as converter input. Documentation will make clear that
`convert-extras` writes a fresh staging directory and does not install or
overwrite Cemu files transactionally. Operators must back up the destination
and install the four card files as one logical group when retaining received
cards and offline partners.

Both root READMEs will include a command reference for every current
subcommand: `inspect`, `inspect-progress`, `inspect-events`, `convert`,
`convert-system`, `convert-extras`, `inspect-cec`, `convert-cec`, `rollback`,
and `rollback-cec`. Each entry will state whether it is read-only, dry-run by
default, write-capable, or destructive; list every positional argument and
option; provide a runnable macOS/Linux shell example; and identify the files it
can read or write. The Windows package README will provide equivalent
PowerShell launcher examples for the common core, system, ExtData, and rollback
flows. Experimental or destructive switches such as `--experimental`,
`--reset-guild-cards`, and `--write` will be explained at the point of use.

## macOS Validation

Validation will not launch Cemu and will not modify an existing MLC. A fresh
temporary root will be created outside the repository. The current local
`user1` sample and Nemessix `user2` source will be copied or opened read-only;
their SHA-256 values will be recorded before and after every test.

For each available slot, the locally built release binary will run:

- `--help` and `inspect`;
- `convert --dry-run` into a same-numbered isolated target path;
- `convert --write` into that isolated path;
- output inspection and expected-size/hash checks;
- reinstall over an isolated existing target to exercise backup creation;
- manifest-bound rollback and byte-for-byte restoration checks.

The shared ExtData directory will run `convert-extras --dry-run` and `--write`
into a new isolated staging directory. The eight expected outputs, sizes, and
source SHA invariance will be checked. CEC will remain read-only unless a
non-empty inbox fixture is explicitly available; no runtime claim will be made
from an empty inbox. Temporary outputs will be retained only as test evidence
or deleted after hashes and results are recorded; no real save is committed.

The README may say "macOS CLI isolated conversion verified" only when all of
these checks pass. It must not say that every gameplay field or Cemu runtime
path was verified by this documentation change.

## Windows Validation and Packaging

The Windows x64 workflow will include the bilingual package README template in
the ZIP and continue producing a statically linked MSVC executable, launcher,
EXE checksum, ZIP checksum, and transactional write/rollback smoke evidence.
The workflow will verify archive contents after extraction, verify the EXE
hash, simulate Mark-of-the-Web, run the launcher, execute a synthetic real
`--write`, and prove rollback restores the original target without changing the
source.

GitHub-hosted Windows CI success permits the documentation to say "Windows x64
package CI verified." It does not prove compatibility with a particular
tester's AppLocker, Smart App Control, antivirus, QQ archive preview, or local
folder permissions. The final Win11 tester run remains a separate runtime gate;
any permission failure must retain the complete operation and path from the CLI
error. Slow unrelated self-hosted jobs are not required for this converter-only
documentation/package verification.

## Verification and Consistency Gates

Before merge:

- all English/Chinese language-switch and relative links resolve;
- command names, options, filenames, sizes, and safety statements agree across
  both language versions and package templates;
- the Windows workflow parses and references the tracked template;
- `git diff --check` passes;
- converter tests and the macOS isolated release-binary matrix pass;
- relevant Windows workflow status is reported exactly, without upgrading a
  queued or failed job into a success claim;
- no real saves, MLC contents, ROMs, absolute user-specific paths, or test
  secrets appear in the commit.

## Delivery

Implementation will remain on `docs/mh3g-bilingual-readme`, be committed and
pushed for review, then merged through a pull request after the documentation,
macOS evidence, and available Windows evidence pass. Existing release archives
and all emulator data remain untouched unless a later packaging task explicitly
rebuilds them.
