## Context

The Raft guest (`crates/chaoscontrol-raft-guest`) runs 3 Raft nodes in-process inside a ChaosControl VM. The pure Raft logic lives in `src/lib.rs` (zero SDK deps, 78 unit tests). The VM entry point `src/main.rs` wires it to the SDK — calling `assert::always/sometimes` after each tick and `random::random_choice` for scheduling decisions.

Current assertion placement:
- 3 `always` in the post-tick sweep: election safety, log matching, leader completeness
- 3 `sometimes` in the post-tick sweep: leader elected, value committed, 3+ committed

All 6 assertions fire at tick granularity from the main loop. No assertions inside handlers, transitions, or at mutation sites. The explorer can't distinguish "handler never called" from "handler called and worked fine" — both produce the same assertion trace.

## Goals / Non-Goals

**Goals:**
- Add assertions inside every message handler entry point so the explorer knows which RPCs are being exercised
- Add sometimes-pairs on binary branches (quorum, timeout, delivery) to detect exploration blind spots
- Add reachable/unreachable on state transitions to confirm the explorer drives the full state machine
- Add inline always-assertions at data mutation points to catch invariant violations at the source
- Keep all new assertions in `main.rs` only — no SDK calls in the pure `lib.rs`

**Non-Goals:**
- Modifying `lib.rs` (the pure Raft logic stays SDK-free and unit-testable)
- Adding new coverage edges (the existing `coverage::record_edge` calls are separate)
- Changing the assertion API (no new SDK functions needed)
- Adding assertions to the net guest or simple guest (those are separate changes)

## Decisions

**1. Assertions in main.rs only, not lib.rs**

The pure/impure boundary is a key design property. `lib.rs` has 78 unit tests that run without a VM. Adding SDK calls there would break that. Instead, assertions go in `main.rs` at the call sites that invoke `lib.rs` functions — right before and after `handle_message`, `become_candidate`, `become_leader`, etc.

Alternative: Add assertion hooks via closures passed into lib.rs. Rejected — adds complexity for no benefit since main.rs already has full visibility into state changes.

**2. Sometimes-pairs, not just sometimes**

For every boolean branch worth exploring, assert both `sometimes(cond, ...)` and `sometimes(!cond, ...)`. A sometimes that never fires across thousands of runs is an exploration quality signal. Single-sided sometimes misses the "explorer never reaches the false branch" case.

**3. json!({}) details on every assertion**

Every assertion includes relevant state: term, node id, commit index, log length. This costs nothing (already serialized in the hypercall) and makes post-mortem triage straightforward. The pattern is already established by the existing 6 assertions.

**4. Group assertions by concern, not by location**

Assertions organized into clear groups: handler reachability (6), state transitions (4+), data invariants (5+), branch coverage (8+). Each group serves a different purpose for the explorer.

## Risks / Trade-offs

**[Hypercall volume]** → Each assertion is a VMCALL. Going from 6 to ~30 assertions per tick increases hypercall overhead by ~5×. At ~1µs per hypercall, this is ~24µs per tick — negligible vs the ~100µs tick budget. If it matters, assertions can be gated behind a compile-time feature flag.

**[Assertion ID collisions]** → The SDK computes assertion IDs via FNV-1a hash of the message string. With 30 assertions the collision risk is near zero, but unique message strings are still required. Each assertion has a distinct descriptive message.

**[False sometimes-failures during short runs]** → A sometimes assertion might not fire in a 500-tick run but would fire in a 5000-tick run. This is by design — it tells the explorer to explore longer or differently. Not a correctness issue.
