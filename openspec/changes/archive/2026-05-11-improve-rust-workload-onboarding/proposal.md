## Why

Rust workload adoption still has too much ceremony: users need to understand local dry-run output, guest packaging, VM campaign commands, replay proof boundaries, and promotion artifacts separately. The current goal is a Rust-only SDK, so onboarding should be polished rather than broadened to other languages.

## What Changes

- **Scaffolded Rust workload path**: Add a template/scaffold flow with explicit local dry-run and VM campaign commands.
- **Promotion checklist**: Provide a deterministic path from local instrumentation to accepted snapshot-backed replay proof.
- **Assertion linting**: Catch weak, uncategorized, or unreachable Rust assertions before a VM campaign.

## Capabilities

### Modified Capabilities
- `rust-workload-harness`: Adds first-class scaffold, CI, and promotion workflows for Rust workloads.

## Impact

- **Files**: SDK harness helpers, templates/docs, local report checker, Nix apps/checks.
- **APIs**: May add public Rust harness builder/config types; existing macros remain compatible.
- **Dependencies**: None expected unless a template generator crate is added.
- **Testing**: SDK unit tests, template dry-run, local report negative fixtures, bounded VM smoke where available.
