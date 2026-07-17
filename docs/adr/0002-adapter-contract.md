# ADR 0002: Versioned adapter contract and evidence-scoped support

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision

`AdapterDescriptor` includes emulator ID, platform, bundle/package/process IDs,
user-root acquisition (`native`, `SAF`, or authenticated IPC), GameKey mapping,
slot mapping, includes/excludes, save-complete/launch/exit capabilities,
stability validator, restore precondition, support level and evidence
fingerprint.

Support is scoped to emulator build + platform + title + region + update + slot.
3G/3U, 4G/4U and XX/GU are distinct GameKeys. Adapters preserve original
formats/paths and exclude shaders, textures, cheats, caches, screenshots and
device configuration by default.

`Runtime Verified` requires a real build and a successful snapshot → mutate or
damage → restore → emulator-readable round trip. Installation, path discovery,
fixtures and HTTP success do not qualify. Fixture-backed or path-only adapters
are `Experimental`.

Android fails closed when SAF or authenticated adapter IPC cannot access another
app's data. Root and Accessibility are never requirements.

## Rollback

An adapter whose evidence fingerprint no longer matches is downgraded and
restore is disabled; local snapshot history remains readable.

