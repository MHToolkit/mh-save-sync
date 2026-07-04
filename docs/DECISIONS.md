# Decision Index

┌──────┬────────────────────────────────────────────┬──────────┐
│ ADR  │ Decision                                   │ Status   │
├──────┼────────────────────────────────────────────┼──────────┤
│ 0001 │ Client stack and shared-core bridge        │ Accepted │
│ 0002 │ Adapter contract and support evidence      │ Accepted │
│ 0003 │ Trigger priority and stability window      │ Accepted │
│ 0004 │ Snapshot, manifest and chunk format        │ Accepted │
│ 0005 │ Recovery secret and device key lifecycle   │ Accepted │
│ 0006 │ PostgreSQL and S3 persistence ordering     │ Accepted │
│ 0007 │ DAG conflicts, restore and tombstones      │ Accepted │
│ 0008 │ Retention and garbage collection           │ Accepted │
│ 0009 │ macOS and Android background policy        │ Accepted │
│ 0010 │ Export bundle and format versioning        │ Accepted │
└──────┴────────────────────────────────────────────┴──────────┘

Entries are Accepted for phase1-alpha implementation authority. Runtime support levels remain evidence-scoped: an accepted ADR does not upgrade any emulator to Runtime Verified without the evidence bundle in `docs/research/EMULATOR_SAVE_MATRIX.md`.

