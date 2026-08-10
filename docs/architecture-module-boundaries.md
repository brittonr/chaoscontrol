# Architecture module boundaries

This map assigns one owner to each migrated state and invariant. A core computes checked plans from supplied facts. A shell performs effects and reports observations.

## VMM ownership

| Module | Owned state and invariants | Inputs | Outputs | Allowed effects | Test boundary |
| --- | --- | --- | --- | --- | --- |
| `vm_core` | VM lifecycle phase, poison state, bounded serial projection, snapshot and teardown decisions | Typed config, current phase, observations, limits | Checked construction, transition, snapshot, poison, and teardown plans | None | Pure unit tests with valid and invalid facts |
| `vm` | KVM VM and vCPU handles, guest memory, timers, threads, devices, eventfds, and applied lifecycle state | Checked `vm_core` plans and host observations | Applied VM effects and typed failures | KVM, memory, timer, thread, and device effects | Existing KVM, poison, timer, snapshot, and teardown tests |
| `controller_core` | Round phase, selected VM order, fault and observation admission, all-or-fail commit decision | VM facts, schedule facts, fault plans, observations | Checked round, fault, observation, and commit plans | None | Pure round and failure-path unit tests |
| `controller` | VM slots, virtual network shell, KVM calls, device mutation, and evidence publication | Checked `controller_core` and `chaoscontrol-sim-core` plans | Applied observations or latched poison | VM, device, KVM, and publication effects | Existing multi-VM, fault, poison, and snapshot tests |
| `unsafe_owner` | Timer identifier transfer and its thread-join lifetime invariant | Created timer ID and scoped owner transfer | Owned timer handle returned to the creating shell | One reviewed `unsafe impl Send` | Timer cancellation, transfer, and teardown tests |

## Evidence ownership

| Module | Owned state and invariants | Inputs | Outputs | Allowed effects | Test boundary |
| --- | --- | --- | --- | --- | --- |
| `replay_readiness_core` | Receipt classification, schema checks, promotion facts, and render models | In-memory JSON or typed facts | Typed decisions and render models | None | Positive, malformed, stale, and overclaim fixtures |
| `replay_readiness_loader` | Confined input loading and parse diagnostics | Explicit paths | Parsed in-memory facts | File reads only | Missing, malformed, and confined-path tests |
| `replay_readiness_orchestration` | Explicit command and follow-up ordering | Checked plans and supplied command results | Execution receipts | Process execution and clock observation | Success, timeout, cancellation, and partial-result tests |
| `replay_readiness_render` | Markdown, HTML, and summary projection from validated render models | Validated render model | Text bytes | None | Golden compatibility fixtures |
| `replay_readiness_publication` | Exact destination and bounded publication sequence | Rendered bytes and explicit path | Publication result | Directory creation and file writes | Exact-byte and invalid-destination tests |

## Dependency direction

The allowed direction is:

```text
pure core -> typed plan or render model -> effect shell -> observation -> pure core
```

Core modules must not use filesystem, environment, process, clock, output, KVM, thread, or ambient mutable state APIs. Evidence renderers must not recompute eligibility. Shell modules must not mutate owned state without a checked plan.

`replay_readiness_surfaces` remains the public compatibility facade. It delegates summary classification, JSON loading, command and clock effects, README rendering, and selected publication paths to the owners above. Existing public paths do not change.

## Compatibility boundary

Public Rust paths remain stable during each slice. JSON field names, enum meanings, error classes, receipt semantics, and deterministic transition results do not change. Temporary re-exports are removed only after all internal call sites use the owning module.

Passing these checks proves only the listed source boundaries and tested behavior. Smaller modules do not prove safety or whole-system correctness.
