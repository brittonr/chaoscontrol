## Context

`OperationHistory` v1 stores one record with invocation and completion timestamps. `SingleRegisterChecker` sorts those records by completion time.

That order is not a valid linearization order for overlapping operations. The v1 completion enum also cannot represent an operation that may have executed after a timeout or disconnect.

The active SMR workload owns chain-specific replica observations. This package owns generic client-event histories and finite model checking.

## Decisions

### 1. Keep semantic histories separate from execution traces

**Choice:** A semantic history records client-visible operations and selected environment events. A replay trace records VMM execution and scheduling.

Reports link both artifacts by identity. They do not merge their claim classes.

**Rationale:** Semantic validity and deterministic reproduction answer different questions.

### 2. Use an event history with explicit pairing

**Choice:** History v2 contains an ordered sequence of typed events. Operation events are `invoke`, `ok`, `fail`, or `info`.

Each operation event binds a stable event index, logical operation ID, attempt ID, process, function, object key, typed value, and controller time.

Completion events pair with one prior invocation. Fault and lifecycle events share the timeline but never enter the object model as client operations.

**Rationale:** Separate events preserve concurrency and exact observed order.

### 3. Preserve uncertain and incomplete outcomes

**Choice:** `ok` must take effect. `fail` must not take effect. `info` may take effect.

An invocation without a completion remains pending. Finalization can convert it to `info` only through an explicit policy and recorded reason.

Retries preserve the logical operation ID and receive distinct attempt IDs.

**Rationale:** Timeout, disconnect, and process loss do not prove non-execution.

### 4. Use canonical BLAKE3 identity for v2

**Choice:** A pure canonical projection uses versioned domains and length-delimited fields. It binds profile, model, event order, event content, bounds, and completeness accounting.

The JSON representation is a transport format. Reordering JSON fields does not change semantic identity.

History v1 retains its existing SHA-256 field for compatibility. A v1 digest cannot satisfy a v2 identity requirement.

**Rationale:** Canonical semantic identity must not depend on serializer field order.

### 5. Implement a bounded linearizability search

**Choice:** The pure checker derives the real-time partial order from operation intervals. It explores legal model transitions that preserve that order.

The search memoizes canonical model state, remaining operations, optional `info` decisions, and predecessor constraints. Profiles name finite operation, state, branch, depth, and memory bounds.

The checker returns:

- `valid` with one legal linearization witness
- `invalid` with a model violation and retained witness history
- `unknown` with an exact search, evidence, or model blocker

Bound exhaustion can never become `valid`.

**Rationale:** A finite search needs an honest terminal state for incomplete evaluation.

### 6. Start with explicit object models

**Choice:** The first models are a read/write register and compare-and-swap register. Each model defines pure initial state and transition functions.

A profile can enable independent-key decomposition only when the model declares key isolation. The checker validates that every operation has one admitted key.

**Rationale:** Decomposition reduces search cost without weakening cross-key models silently.

### 7. Retain witnesses and bounded reductions

**Choice:** A valid report contains one linearization order. An invalid report contains the failing operation set, model states, and violated transition.

A pure reducer removes events while preserving well-formed pairing, profile validity, and the same invalid failure class. It records whether the result is minimal, locally reduced, or budget-limited.

**Rationale:** Small, typed witnesses make failures reviewable and reproducible.

### 8. Use an independent reference oracle

**Choice:** The shell can export admitted histories to a pinned Jepsen-compatible format. A conformance rail compares native verdicts with an independent reference checker.

The corpus includes valid sequential histories, valid overlapping histories, stale reads, compare-and-swap conflicts, indefinite outcomes, pending operations, malformed pairs, and bound exhaustion.

A disagreement blocks promotion and preserves both reports. The reference tool remains external and optional outside the conformance rail.

**Rationale:** Differential validation can expose checker defects without adding a product runtime dependency.

### 9. Render semantic timelines from one pure projection

**Choice:** A pure report projection joins operation intervals, outcomes, applied and observed fault windows, lifecycle phases, latency, and witness membership.

Text, JSON, and static HTML renderers consume the same projection. Renderers cannot change verdicts or infer missing events.

**Rationale:** One semantic projection prevents dashboard and CLI drift.

### 10. Keep the core pure and the shell thin

**Choice:** Event admission, pairing, canonicalization, model transitions, search, verdict classification, reduction, and timeline projection remain pure.

File reads, SDK transport, VMM control, external checker execution, persistence, and rendering remain imperative shells.

**Rationale:** Core behavior must run against in-memory positive and negative fixtures.

## Dependencies and sequencing

`verify-fault-application-outcomes` must define applied and observed fault records before v2 reports can make fault-effect claims.

The checker core, history schema, and non-fault fixtures can proceed first. The active SMR change remains the owner of chain validation and can add a later v2 projection.

## Risks and trade-offs

- Linearizability search can grow quickly. Named bounds and independent-key decomposition keep cost explicit.
- A checker defect can produce false verdicts. Differential fixtures reduce this risk but do not prove soundness.
- `info` outcomes enlarge the search space. Profiles must bound them separately.
- Timeline output can imply causation. Reports show temporal overlap and never infer causation from proximity alone.
