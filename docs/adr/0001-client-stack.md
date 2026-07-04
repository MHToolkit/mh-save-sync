# ADR 0001: Shared Rust core with native platform shells

- Status: Accepted for phase1-alpha
- Date: 2026-07-04
- Owners: MHToolkit maintainers
- Review date: 2026-10-04

## Decision question

How can macOS and Android share every data-integrity decision while retaining
correct platform storage, background and key-store integration?

## Evidence

See `../RESEARCH_PLAN.md#r6-clientplatform-feasibility`. A thin bridge PoC must
build and invoke the same snapshot/conflict functions from Swift and Kotlin
before this ADR becomes Accepted.

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

Rust workspace builds and tests shared domain/crypto/engine/client/server/CLI. macOS SwiftPM shell smoke builds and invokes a native status surface. Android Kotlin shell records SAF/WorkManager policy skeleton. UniFFI remains the bridge target for the next implementation slice.
