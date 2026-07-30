## Context

The schedule engine currently returns `Vec<Fault>` and increments its injected counter before the controller sees the targets. `step_round` copies that vector to `faults_fired`, then `apply_fault` returns only `Result<(), VmError>`. Most `if let Some(slot)` misses return success. Some device misses log warnings and still return success. Several apparent application fields are only reset, expired, or snapshotted and are not consulted by the relevant device, scheduler, allocation, or time path.

The runtime needs a stage model that separates deterministic selection from effect evidence.

## Decisions

### 1. Give each attempt explicit stages and identity

**Choice:** A canonical `FaultAttempt` binds the run/schedule identity, selection position, normalized fault, target, and seed-derived context with BLAKE3. The lifecycle is explicit:

1. `Selected`: the deterministic schedule made the fault due.
2. `Applicable`: a pure planner validated targets, parameters, capabilities, and current simulation facts and produced an effect plan.
3. `Applied`: the imperative adapter completed the declared immediate action or installed a mechanism that the real execution/data path consumes.
4. `Observed`: that path later recorded the specific guest-visible or simulation-visible effect.

A failed applicability check produces `Rejected`; an imperative failure produces `ApplicationFailed`. Neither is applied or observed.

**Rationale:** One attempt identity lets reports correlate stages without collapsing their meaning.

### 2. Plan applicability in a functional core

**Choice:** A pure planner consumes a `Fault`, normalized simulation facts, and explicit policy. Facts include VM existence/status, vCPU topology, device kinds and capabilities, address/range bounds, and supported effect adapters. It returns a typed `FaultPlan` or a stable rejection reason.

The planner validates target indices, vCPU indices, register bits, rates, durations, arithmetic, storage ranges, device presence, and implementation support before any mutation.

**Rationale:** Target lookup mixed into mutation creates silent no-ops and makes negative cases hard to exhaust.

### 3. Require a reachable enforcement path for every applied variant

**Choice:** A registry maps each public fault variant to its planner, shell adapter, and optional observation hook. A variant can be advertised as supported only when the actual block, network, scheduler, clock, CPU, process, interrupt, or resource path consumes the applied state. Merely assigning a field that no path reads cannot produce `Applied`.

Currently inert behaviors must be wired to their named path or explicitly rejected as unsupported. For example, disk errors/full state must be consumed by the block backend; vCPU stall by scheduling; freeze/jitter by virtual-time reads/advancement; and memory pressure by an actual bounded resource mechanism.

**Rationale:** This creates a finite conformance matrix and prevents placeholder state from becoming false evidence.

### 4. Return outcomes from the imperative shell

**Choice:** Applying a plan returns a typed record naming the effect mechanism and whether it is immediate or armed for a future trigger. `Applied` is emitted only after all required operations succeed. Multi-operation adapters either roll back, remain non-runnable with an indeterminate failure, or return a typed partial failure; they never publish success after partial mutation.

`step_round` preserves the ordered outcome sequence even when a later attempt fails and does not pre-populate a fired list.

**Rationale:** `Result<()>` cannot express no-op, unsupported, armed, immediate, or observed distinctions.

### 5. Observe effects at the real consumption point

**Choice:** Device and execution paths emit `FaultObservation` when an armed mechanism changes a concrete operation: a packet is dropped/delayed/corrupted, a block operation returns or mutates differently, a vCPU is skipped, virtual time is frozen/jittered, a memory request is denied, or an interrupt/CPU/process action completes. Observations bind the attempt identity and deterministic operation identity.

Effects with no later trigger may be applied but unobserved. Reports state that distinction instead of upgrading application to observation.

**Rationale:** Installing a fault policy does not prove the workload exercised it.

### 6. Make accounting and compatibility unambiguous

**Choice:** Engine snapshots and reports carry separate selected, rejected, applied, application-failed, and observed counters plus ordered outcome records. Counter transitions use checked or explicitly saturating arithmetic and are derived from accepted transitions. Legacy names, if retained temporarily, are documented aliases for one stage and never span stages.

Runtime records remain Rust-owned. Compact review summaries and readiness contracts may validate the new schema at the Nickel boundary.

**Rationale:** Downstream exploration, minimization, and replay require stable semantics more than a familiar field name.

### 7. Prove every variant's positive or negative behavior

**Choice:** A table-driven conformance suite covers every public `Fault` variant. Supported variants have a positive effect-path test and, when applicable, a later observation test. All variants have negative invalid-target/parameter/capability tests. Replay compares the full ordered stage trace, and snapshot tests prove pending armed mechanisms and counters resume consistently.

**Rationale:** Spot tests for a few block variants would leave the same class of bug in untested arms.

## Risks / Trade-offs

- Stage-specific records expand report and snapshot schemas, but remove materially misleading shorthand.
- Some advertised variants may become explicit unsupported errors until a real enforcement mechanism exists.
- Observing effects adds hooks on hot paths; records should remain bounded and avoid allocating from guest-controlled values.
- Application proves only the declared mechanism succeeded; it is not evidence that the workload reached or was harmed by that mechanism.
