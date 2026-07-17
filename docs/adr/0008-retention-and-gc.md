# ADR 0008: Tiered retention and conservative mark-and-sweep

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

Default retention keeps the newest 20 snapshots, 14 daily, 8 weekly and 12
monthly restore points per logical save. Pinned snapshots, active HEADs,
conflict branches, restore safeguards and their required ancestors are never
automatically removed.

Retention first records tombstones. Physical deletion is a separate
mark-and-sweep over all live roots with a grace period. Object deletion is
idempotent and auditable. A snapshot or chunk newly uploaded, leased or
referenced during the sweep is protected by generation/lease checks.

Quota pressure may pause uploads and suggest cleanup; it may not delete the only
known recoverable version or mutate emulator-local files.

