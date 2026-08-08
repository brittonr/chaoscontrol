## Why

ChaosControl now has accepted snapshot-backed replay proof for the in-tree Rust workload, but a downstream Rust team still needs a clearer golden path before it feels like an Antithesis alternative: copy a template, run a local instrumentation smoke, inspect registered-vs-observed assertions, then decide whether to spend VM/replay budget.

## What Changes

- Add a copyable Rust workload template with README, Cargo manifest, and harness entrypoint.
- Extend the local SDK report with explicit registered/observed/unobserved per-assertion coverage details while preserving the existing summary fields.
- Document the golden path from template copy to local dry-run to bounded VM/replay proof.

## Capabilities

### Modified Capabilities
- `rust-workload-harness`: strengthens downstream onboarding and local instrumentation quality reporting.

## Impact

- **Files**: SDK workload report parser, local report summarizer, docs/template files, README/harness docs, OpenSpec artifacts.
- **APIs**: additive report fields only; existing summary keys remain stable.
- **Testing**: targeted Rust SDK tests, Python summarizer smoke, strict OpenSpec validation, whitespace check.
