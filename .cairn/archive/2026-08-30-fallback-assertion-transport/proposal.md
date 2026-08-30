# Fallback Assertion Transport

## Why

The SDK is Rust-only. A C, C++, Go, or unlinked binary cannot use the hypercall transport or the assertion catalog. Antithesis solved this with a fallback JSONL scheme that any process can emit without SDK linkage. ChaosControl's property oracle receives all its evidence through the Rust SDK, so third-party binaries produce no assertion evidence at all.

## What Changes

- Define a stable, language-agnostic assertion and lifecycle record format that any process can write without linking the SDK.
- Ingest records deterministically into the existing property oracle and catalog identity rules.
- Bind a fallback record to an owning process identity and to its catalog fingerprint.
- Keep coverage separate: coverage still requires the SanCov hooks, which stay in the Rust SDK or an equivalent host-side collector.

## Impact

- **Protocol**: a fallback record schema compatible with the assertion catalog.
- **Evidence**: fallback records enter bug reports and replay verdicts with process identity.
- **Testing**: positive ingestion and negative malformed, unbounded, and identity-conflict cases.

## Non-Goals

- No new assertion kinds.
- No coverage scheme that works without instrumentation.
- No hosted report surface.
