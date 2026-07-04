# Governance

## Ownership

`MHToolkit/maintainers` has maintain permission. `main` is bootstrap/review
only; implementation is developed through pull requests. CODEOWNERS review is
required for crypto, domain/protocol, engine, deployment, workflows and ADRs.

On 2026-07-04 the GitHub branch-protection API returned HTTP 403 for this
private repository with the explicit requirement to upgrade the account plan or
make the repository public. The repository remains private; therefore the
review-only rule is currently a documented process control and CI/PR evidence
gate rather than an enforceable GitHub branch rule. Recheck protection when the
organization plan changes.

## Decisions

Changes to protocols, persisted formats, cryptography, compatibility claims,
repository boundaries, platform background behavior, storage or performance
budgets require an ADR with evidence, migration and rollback.

## Evidence levels

- `Catalogued`: identity only.
- `Experimental`: contract or fixture-backed implementation without full real
  runtime proof.
- `Runtime Verified`: reproducible real build/device/save/restore proof with a
  current fingerprint.
- `Unsupported`: known unsafe or unavailable path.

Runtime Verified is per emulator, platform, title, region, update and slot. It
never implies cross-region conversion.

## Incident policy

P0 includes save loss/corruption, key disclosure, remote execution or broad
unrecoverability. Stop affected releases/synchronization immediately, preserve
evidence and publish a timeline, impact, root cause, detection gap, corrective
tests and owner. “Next time be careful” is not corrective action.

## Privacy

Issues, PRs, CI artifacts and diagnostics must use synthetic data and redacted
paths. Secrets live outside the repository. Production credentials use short
lived identity or secret files, never examples or source.
