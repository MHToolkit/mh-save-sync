# ADR 0001: Shared Rust core with native platform shells

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision question

How can macOS and Android share every data-integrity decision while retaining
correct platform storage, background and key-store integration?

## Evidence

See `../RESEARCH_PLAN.md#r6-clientplatform-feasibility`.

Phase1-alpha bridge evidence from 2026-07-05:

- `cargo build --release -p save-client` produced a shared client library
  exposing versioned UniFFI DTOs and bridge functions backed by the shared Rust
  conflict engine.
- `cargo run -p save-client --features bindgen --bin uniffi-bindgen -- generate
  target/release/libsave_client.dylib --library --crate save_client --language
  kotlin --out-dir artifacts/uniffi/kotlin` generated
  `artifacts/uniffi/kotlin/uniffi/save_client/save_client.kt`
  (`sha256:dd579e3f4b47cfbd8e91d326b55be2f72cff3a74ec34faee9227faceec99edc8`).
- The matching Swift binding generated `save_client.swift`
  (`sha256:6f9d6af05b44b02cd72d69e22ed9448c1f76732ddf06f2989d3ba0823d2cb9b1`)
  and FFI headers. The macOS shell still uses a native smoke surface; full
  Swift integration remains a Phase 1C task.
- Android Kotlin/Compose shell builds and lints with SAF persistence,
  WorkManager scheduling and foreground-session service scaffolding.

## Decision

Use Rust for domain, crypto, adapters, snapshot engine, protocol and client
orchestration. Use SwiftUI/AppKit for macOS and Kotlin/Compose for Android.
Generate UniFFI bindings for coarse, versioned operations and serializable DTOs.
Platform shells own Keychain/Keystore, SAF, notifications, background scheduling
and process APIs; they do not reimplement synchronization or conflict rules.

If UniFFI cannot satisfy a measured packaging/runtime constraint, a narrow
versioned C ABI with generated Swift/Kotlin wrappers may replace only the bridge.
That change requires PoC evidence and a superseding ADR.

## Alternatives

- Shared WebView UI: rejected because storage, process lifecycle, background
  execution and key stores require native integration.
- Two native synchronization engines: rejected because behavior drift creates a
  direct data-loss risk.

## Migration and rollback

Bridge DTOs have an independent major/minor version. A bridge failure disables
cloud actions but leaves local emulator files untouched.
## Phase1-alpha evidence

Rust workspace builds and tests shared domain/crypto/engine/client/server/CLI.
macOS SwiftPM shell smoke builds and invokes a native status surface. Android
Kotlin shell records SAF/WorkManager/foreground-service policy behavior.
UniFFI binding generation is now CI-gated for Kotlin and Swift, but full native
calls through the generated bridge remain an open Phase 1C integration gate.
