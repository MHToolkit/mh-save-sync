# MH Save Sync Agent Instructions

## Command discipline

- Prefix every shell command with `rtk`; prefix every command-chain segment.
- If `.codegraph/` exists, use CodeGraph before grep/find/file reads when
  locating or understanding code.
- Run `git diff --check` and relevant tests before claiming completion.

## Data integrity

- Never implement silent last-write-wins.
- Never upload directly from a watcher event.
- Never restore into a running emulator.
- Never move or normalize the emulator's original save data.
- Every restore first snapshots the current state and must support rollback.
- Runtime Verified requires reproducible real-device evidence.

## Security and privacy

- Do not commit real saves, recovery phrases, device keys, tokens, credentials,
  decrypted paths/content, ROMs, firmware or console keys.
- Use deterministic synthetic fixtures.
- Store local secrets only under `~/Documents/Secrets` with mode `0600`.
- Material protocol, format, crypto, storage or platform-policy changes require
  an ADR and migration/rollback notes.

## Delivery

- `main` receives bootstrap and reviewed PRs only.
- Feature implementation belongs on `feat/phase1-save-sync`.
- Keep research claims linked to official sources, access dates, reproduction
  commands, evidence and explicit adopt/reject decisions.

