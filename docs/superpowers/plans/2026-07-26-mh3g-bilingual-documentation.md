# MH3G Converter Bilingual Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish complete English and Simplified Chinese usage documentation for the Japanese MH3G converter, package it with reproducible macOS/Windows CLI artifacts, and prove the documented macOS flows against isolated real-save copies.

**Architecture:** Keep the English and Chinese repository guides in separate, cross-linked files, while archive-local package guides are bilingual in one file. Treat core slots, shared `system`, shared ExtData, and NAND CEC as four explicit input groups; never imply whole-folder or ZIP auto-discovery. A small documentation contract test prevents command/path drift, and platform validation distinguishes local macOS evidence, GitHub-hosted Windows evidence, and later Win11 tester evidence.

**Tech Stack:** Markdown, Python 3 standard library, Bash/zsh, PowerShell, GitHub Actions YAML, Rust/Cargo, SHA-256, macOS `ditto`/`shasum`.

---

## File Map

- Create `README.zh-CN.md`: complete Simplified Chinese mirror of the root guide.
- Modify `README.md`: English language switch, exact input-shape matrix, complete command reference, and verified platform status.
- Create `docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md`: Chinese mirror of the exact component/read-write contract.
- Modify `docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md`: language switch and clarified ExtData/CEC input levels.
- Create `packaging/mh3g-save-convert/README-Windows.txt`: Chinese-first bilingual Windows archive guide.
- Create `packaging/mh3g-save-convert/README-macOS.txt`: Chinese-first bilingual macOS archive guide.
- Modify `.github/workflows/mh3g-converter-windows.yml`: copy the tracked Windows guide and trigger when package documentation changes.
- Create `scripts/package-mh3g-macos.sh`: reproducibly build and package the arm64 macOS CLI with its guide and checksums.
- Create `scripts/mh3g-docs-contract.py`: validate language links, input terminology, command coverage, package-template references, and forbidden ZIP claims.
- Modify `scripts/verify-local.sh`: invoke the fast documentation contract check.
- Do not modify emulator saves, MLC contents, ROMs, `deploy/compose/README.md`, or engineering research outside the exact links needed by the player guide.

### Task 1: Add a Failing Documentation Contract

**Files:**
- Create: `scripts/mh3g-docs-contract.py`
- Modify: `scripts/verify-local.sh`

- [ ] **Step 1: Create the contract checker with the final required file and command lists**

Implement a Python 3 script whose constants include:

```python
ROOT_DOCS = ("README.md", "README.zh-CN.md")
CONTRACT_DOCS = (
    "docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md",
    "docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md",
)
PACKAGE_DOCS = (
    "packaging/mh3g-save-convert/README-Windows.txt",
    "packaging/mh3g-save-convert/README-macOS.txt",
)
COMMANDS = (
    "inspect",
    "inspect-progress",
    "inspect-events",
    "inspect-cec",
    "convert",
    "convert-system",
    "convert-extras",
    "convert-cec",
    "rollback",
    "rollback-cec",
)
```

The script must exit nonzero with one line per failure when a required file is
missing, a root/contract language counterpart is not linked, a root guide omits
one of the ten commands, either root guide omits both exact input suffixes
`extdata/00000000/00000481/user/` and `CEC/00048100/`, or either root guide
claims that ZIP is accepted directly. It must also parse
`.github/workflows/mh3g-converter-windows.yml` as text and require the tracked
Windows template path.

- [ ] **Step 2: Run the checker and prove that the current repository fails**

Run:

```bash
rtk python3 scripts/mh3g-docs-contract.py
```

Expected: nonzero exit listing at least missing `README.zh-CN.md`, missing
Chinese contract, and missing package guides.

- [ ] **Step 3: Wire the checker into the local verification entry point**

Add this command near the other fast policy checks in `scripts/verify-local.sh`:

```bash
python3 scripts/mh3g-docs-contract.py
```

Do not prefix commands inside the repository script with `rtk`; RTK is an
interactive-agent wrapper and is not a runtime dependency of the project.

- [ ] **Step 4: Validate Python syntax and commit the failing contract**

Run:

```bash
rtk python3 -m py_compile scripts/mh3g-docs-contract.py
rtk git diff --check
```

Expected: Python compilation and whitespace checks pass; the contract itself
still fails because subsequent documentation tasks have not created its inputs.

Commit:

```bash
rtk git add scripts/mh3g-docs-contract.py scripts/verify-local.sh
rtk git commit -m "test(mh3g): define bilingual documentation contract"
```

### Task 2: Complete the English Player Guide

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add the language selector and scope warning**

Immediately below `# MH Save Sync`, add:

```markdown
[English](README.md) | [简体中文](README.zh-CN.md)
```

Keep the repository-wide alpha warning. In the MH3G section, state prominently
that the converter supports Japanese MH3G only, is 3DS-to-Cemu only, reads no
ZIP/7z/RAR directly, does not recursively discover files, and requires full
archive extraction before use.

- [ ] **Step 2: Add the exact four-group input table**

Add a table before the first conversion command with these rows:

```text
Core slot       one explicit user1/user2/user3 file
Shared system   one explicit system file
Shared ExtData  exact .../00000481/user directory with all eight card*/quest* files
StreetPass CEC  exact .../CEC/00048100 directory with InBox___
```

For every row, include accepted input, rejected input, required/optional status,
and the files affected. Explain that `00000481` is the ExtData root but
`00000481/user` is the `convert-extras` input.

- [ ] **Step 3: Replace the partial converter walkthrough with a complete command reference**

For each of the ten commands, provide:

- exact syntax copied from current `--help`;
- positional arguments and every option;
- read-only/dry-run/write/rollback classification;
- one quoted-path shell example;
- exact read/write boundary and expected transaction artifacts.

Explicitly document `--quest-id`, `--all`, `--target`, `--source-slot`,
`--slot`, `--experimental`, `--reset-guild-cards`, `--dry-run`, `--write`, and
`--manifest`. State that `--write` conflicts with `--dry-run`, CEC writes require
`--experimental`, output slot basename must match the source slot basename, and
`convert-extras` refuses existing named outputs.

- [ ] **Step 4: Clarify durable guild cards versus CEC**

Document this exact durable migration set:

```text
matching user# + card1 + card2 + card3 + cardbox
```

Define `InBox___`, `OutBox__`, `BoxInfo_____`, and `_*`; state that only
non-empty inbox messages are candidate imports, outbox is intentionally ignored,
and an empty inbox can coexist with durable received cards. Keep
`convert-cec` explicitly experimental and independent.

- [ ] **Step 5: Add platform/package status without overstating evidence**

Add sections for macOS arm64 and Windows x64. The macOS claim must remain
"pending isolated CLI validation" until Task 6 passes. The Windows claim must
say that the GitHub-hosted package workflow verifies hash, launcher, synthetic
write, and rollback, while Win11 application-control behavior remains a tester
gate. Preserve the warning to run from a fully extracted local folder.

- [ ] **Step 6: Check command drift and commit**

Run all help surfaces:

```bash
rtk cargo run --quiet -p mh3g-save-convert -- --help
rtk cargo run --quiet -p mh3g-save-convert -- convert --help
rtk cargo run --quiet -p mh3g-save-convert -- convert-system --help
rtk cargo run --quiet -p mh3g-save-convert -- convert-extras --help
rtk cargo run --quiet -p mh3g-save-convert -- inspect-cec --help
rtk cargo run --quiet -p mh3g-save-convert -- convert-cec --help
rtk git diff --check
```

Expected: documented commands/options match help; whitespace check passes.

Commit:

```bash
rtk git add README.md
rtk git commit -m "docs(mh3g): complete English converter guide"
```

### Task 3: Add the Complete Simplified Chinese Guide

**Files:**
- Create: `README.zh-CN.md`

- [ ] **Step 1: Translate the complete root README rather than only the converter section**

Create `README.zh-CN.md` with the same heading order, commands, paths, tables,
links, status claims, and safety invariants as `README.md`. Translate prose and
UI descriptions into Simplified Chinese; preserve command names, filenames,
hash names, environment variables, URLs, title IDs, offsets, and code blocks
verbatim. Put this selector below the title:

```markdown
[English](README.md) | [简体中文](README.zh-CN.md)
```

- [ ] **Step 2: Make the converter input warning Chinese-first and explicit**

Use unambiguous wording equivalent to:

```text
当前 CLI 不支持直接读取 ZIP、7z、RAR，也不会从整个存档目录中自动寻找 user#。
请先完整解压，再按命令要求选择一个具体文件或指定层级的目录。
```

Include the same four-group table and ten-command reference as English. Explain
Savedata, SD-card ExtData, and NAND CEC as three distinct storage areas.

- [ ] **Step 3: Check structural parity and commit**

Run:

```bash
rtk proxy rg '^#{1,4} ' README.md
rtk proxy rg '^#{1,4} ' README.zh-CN.md
rtk git diff --check
```

Expected: both guides cover the same top-level sections and converter
subcommands; whitespace check passes.

Commit:

```bash
rtk git add README.zh-CN.md
rtk git commit -m "docs(mh3g): add Simplified Chinese project guide"
```

### Task 4: Publish the Exact File Contract in Both Languages

**Files:**
- Modify: `docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md`
- Create: `docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md`

- [ ] **Step 1: Add cross-language navigation and exact directory-level rules to English**

Add the English/Chinese selector at the top. Extend the source-location table to
distinguish the ExtData root from its required `user` child and the CEC mailbox
root from `InBox___` records. Add an explicit "Archive Inputs" subsection saying
ZIP/7z/RAR are unsupported and must be extracted first.

- [ ] **Step 2: Create a complete Chinese contract mirror**

Translate every section and table, including accepted sizes, generated sizes,
component groups, command read/write boundaries, backup/manifest names, files
not automatically modified, and implementation evidence. Preserve all byte
sizes, offsets, filenames, CLI identifiers, and source-code paths exactly.

- [ ] **Step 3: Compare invariant tokens across both contracts**

Run:

```bash
rtk proxy rg -o '0x[0-9A-Fa-f]+|user[123]|cardbox|quest[1-4]|convert-[a-z-]+' docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md | rtk proxy sort -u
rtk proxy rg -o '0x[0-9A-Fa-f]+|user[123]|cardbox|quest[1-4]|convert-[a-z-]+' docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md | rtk proxy sort -u
rtk git diff --check
```

Expected: invariant identifier sets match and whitespace check passes.

- [ ] **Step 4: Commit the bilingual contract**

```bash
rtk git add docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md
rtk git commit -m "docs(mh3g): publish bilingual file contract"
```

### Task 5: Track Bilingual Package Guides and Reproducible Packaging

**Files:**
- Create: `packaging/mh3g-save-convert/README-Windows.txt`
- Create: `packaging/mh3g-save-convert/README-macOS.txt`
- Modify: `.github/workflows/mh3g-converter-windows.yml`
- Create: `scripts/package-mh3g-macos.sh`

- [ ] **Step 1: Write Chinese-first bilingual archive guides**

Each guide must contain Chinese and English sections with: Japanese-profile-only
scope, full extraction requirement, source/input matrix, `--help`, `inspect`,
dry-run, write, ExtData, and rollback examples, process-stop warning, source
read-only guarantee, transaction artifacts, and checksum verification. The
Windows examples must invoke `Run-Converter.ps1`; macOS examples must invoke
`./mh3g-save-convert` and explain `chmod +x` only as recovery for permission bits
lost by third-party transfer tools.

- [ ] **Step 2: Replace inline Windows README generation with a tracked copy**

In the Windows package step, replace the PowerShell here-string with:

```powershell
Copy-Item "packaging/mh3g-save-convert/README-Windows.txt" "$stage/README-Windows.txt"
```

Add `packaging/mh3g-save-convert/**` to the pull-request and push path filters.
Retain EXE checksum verification, Mark-of-the-Web simulation, `--help`, real
synthetic `--write`, and rollback checks.

- [ ] **Step 3: Add a macOS arm64 packaging script**

Implement `scripts/package-mh3g-macos.sh` with `set -euo pipefail`. It must:

1. require `uname -m` to report `arm64`;
2. run `cargo build --locked --release -p mh3g-save-convert --bin mh3g-save-convert`;
3. create a fresh staging directory named `mh3g-save-convert-macos-arm64`;
4. copy the binary and tracked macOS README;
5. ensure the binary is executable and run `--help`;
6. create `mh3g-save-convert.sha256` inside the stage;
7. archive the parent directory using macOS `ditto -c -k --keepParent`;
8. create a ZIP SHA-256 sidecar;
9. extract the ZIP into a fresh verification directory, verify the inner hash,
   and run the extracted binary's `--help`.

Accept an optional output directory as the only positional argument, defaulting
to `artifacts/`. Never read or package a save file.

- [ ] **Step 4: Run package-static validation and commit**

Run:

```bash
rtk python3 scripts/mh3g-docs-contract.py
rtk proxy ruby -e 'require "yaml"; YAML.load_file(".github/workflows/mh3g-converter-windows.yml"); puts "yaml-ok"'
rtk proxy bash -n scripts/package-mh3g-macos.sh
rtk git diff --check
```

Expected: documentation contract, YAML parsing, shell syntax, and whitespace
checks pass.

Commit:

```bash
rtk git add packaging/mh3g-save-convert .github/workflows/mh3g-converter-windows.yml scripts/package-mh3g-macos.sh scripts/mh3g-docs-contract.py scripts/verify-local.sh
rtk git commit -m "build(mh3g): package bilingual converter guides"
```

### Task 6: Run Isolated macOS Real-Save Validation

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Generate outside git: temporary validation directory and local evidence log

- [ ] **Step 1: Prove all emulator processes are stopped and capture source hashes**

Use these read-only sources if present:

```text
/Users/vincentadamnemessis/Downloads/user1
/Users/vincentadamnemessis/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/00000000000000000000000000000000/00000000000000000000000000000000/title/00040000/00048100/data/00000001/user2
/Users/vincentadamnemessis/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/00000000000000000000000000000000/00000000000000000000000000000000/extdata/00000000/00000481/user
```

Run `pgrep -ifl 'Cemu|Azahar|Nemessix'` and stop without validation if any
emulator is running. Record `shasum -a 256` for both slots and every ExtData
component. Do not write beside a source file.

- [ ] **Step 2: Build and package the release CLI**

Run:

```bash
rtk ./scripts/package-mh3g-macos.sh /tmp/mh3g-converter-package-20260726
```

Expected: executable, inner checksum, ZIP, ZIP sidecar, extracted hash check,
and `--help` smoke all pass.

- [ ] **Step 3: Validate `user1` and `user2` only in isolated directories**

Create `/tmp/mh3g-converter-validation-20260726/{user1,user2}`. For each source,
run the packaged release binary's `inspect`, `inspect-progress`, `inspect-events`,
`convert --dry-run`, and `convert --write` against an output with the same
basename. Require source size `0x8A00` (35328 bytes), output size `0x8A24`
(35364 bytes), a converter manifest after write, and a source hash identical to
its preflight value.

Run a second `--write` against the isolated output to exercise reinstall
history, then run manifest-bound `rollback`. Require rollback success and no
source hash change. Never point `--output` at a Cemu MLC.

- [ ] **Step 4: Validate ExtData into a fresh staging directory**

Run `convert-extras --dry-run` and `convert-extras --write` with the real
ExtData `user` directory and an empty `/tmp` output directory. Require exactly
eight output files with sizes:

```text
card1/card2/card3  0x58024
cardbox            0x30024
quest1..quest4     0x29024
```

Recompute every source SHA-256 and require exact equality with preflight.

- [ ] **Step 5: Run read-only CEC inspection if the mailbox exists**

Run `inspect-cec` against the NAND mailbox. Do not run `convert-cec --write`
unless a non-empty inbox is present and a separate experimental test is
explicitly authorized. Record inbox/outbox counts without claiming runtime
guild-card validation.

- [ ] **Step 6: Update platform status from pending to the exact passed evidence**

Only after Steps 1-5 pass, update both root guides to state that macOS arm64
release packaging, real `user1`/`user2` isolated dry-run/write/rollback, and
real ExtData staging passed on 2026-07-26. State explicitly that Cemu was not
launched and no gameplay/runtime claim was added.

- [ ] **Step 7: Commit the evidence-backed wording**

```bash
rtk git add README.md README.zh-CN.md
rtk git commit -m "docs(mh3g): record isolated macOS converter validation"
```

### Task 7: Final Verification, Pull Request, and Merge

**Files:**
- Verify all files changed in Tasks 1-6

- [ ] **Step 1: Run focused local gates**

```bash
rtk python3 scripts/mh3g-docs-contract.py
rtk cargo fmt --all -- --check
rtk cargo test --locked -p mh3g-save-convert
rtk cargo clippy --locked -p mh3g-save-convert --all-targets -- -D warnings
rtk proxy ruby -e 'require "yaml"; YAML.load_file(".github/workflows/mh3g-converter-windows.yml"); puts "yaml-ok"'
rtk proxy bash -n scripts/package-mh3g-macos.sh
rtk git diff --check main...HEAD
```

Expected: every command exits zero.

- [ ] **Step 2: Audit for private artifacts and unsupported claims**

Run:

```bash
rtk proxy rg -n '/Users/vincentadamnemessis|Library/Application Support/Nemessix Dev|\.wua|recovery-secret' README.md README.zh-CN.md docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT*.md packaging scripts/mh3g-docs-contract.py
rtk proxy rg -n '(ZIP|zip).*(directly supported|直接支持|直接读取成功)' README.md README.zh-CN.md packaging docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT*.md
rtk git status --short
```

Expected: no private absolute paths, ROM paths, secrets, direct-ZIP support
claims, untracked saves, or generated package binaries in git.

- [ ] **Step 3: Push and open the pull request**

```bash
rtk git push -u origin docs/mh3g-bilingual-readme
rtk gh pr create --base main --head docs/mh3g-bilingual-readme --title "docs(mh3g): publish bilingual converter guide" --body-file /tmp/mh3g-bilingual-pr.md
```

The PR body must list files added, exact macOS isolated evidence, source-hash
invariance, Windows workflow changes, and the outstanding Win11 tester gate.

- [ ] **Step 4: Inspect GitHub checks without waiting on unrelated self-hosted jobs**

Run:

```bash
rtk gh pr checks --watch --fail-fast
rtk gh run list --workflow mh3g-converter-windows --branch docs/mh3g-bilingual-readme --limit 3
```

Wait for the GitHub-hosted Windows converter workflow. Report unrelated slow
self-hosted jobs separately and do not treat them as converter-package evidence.
If Windows fails, inspect its exact failed step before changing code.

- [ ] **Step 5: Review and merge**

Review `main...HEAD` for command drift, mistranslated safety semantics, private
paths, workflow/package mismatch, and evidence overstatement. After required
checks and review pass:

```bash
rtk gh pr merge --squash --delete-branch
rtk git switch main
rtk git pull --ff-only origin main
rtk git status --short --branch
```

Expected: PR merged, local `main` equals `origin/main`, and worktree is clean.

- [ ] **Step 6: Hand off the Win11 tester gate**

Provide the workflow artifact name, commit SHA, ZIP SHA-256, EXE SHA-256, and
PowerShell launch command. Ask the tester to report the complete error line if
Windows still returns permission denied. Do not call Windows Runtime Verified
until the extracted launcher performs at least `--help`, `inspect`, dry-run,
isolated `--write`, and rollback on the tester's x64 Win11 machine.
