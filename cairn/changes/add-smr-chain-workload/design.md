## Context

ChaosControl already controls entropy, scheduling, faults, snapshots, and replay for selected Rust guests. Its Raft guest has protocol-specific assertions and accepted bounded replay evidence.

The missing layer is a reusable semantic workload. This workload must observe committed application state without inspecting a specific consensus implementation.

## Decisions

### 1. Keep the workload independent of the consensus algorithm

**Choice:** A consumer adapter exposes bounded proposal, committed-transition observation, lifecycle, and terminal-status operations. The workload does not inspect terms, elections, quorum internals, or protocol messages.

**Rationale:** The same workload can exercise Raft, Paxos, Viewstamped Replication, or another admitted SMR implementation.

### 2. Use one canonical BLAKE3 transition

**Choice:** The chain state is a command count and a digest. Genesis and transition hashes use separate versioned domains.

The genesis input contains the profile ref and canonical initial-state ref. Each transition contains the profile ref, command index, prior digest, command length, and command bytes. Fixed-width integers use network byte order.

**Rationale:** Domain separation and explicit framing prevent ambiguous concatenation. BLAKE3 matches stack-owned content identity.

### 3. Compare replica states by command index

**Choice:** A pure validator groups observations by workload profile and command index. Different digests or application-state refs at one index are a safety violation.

A lagging replica or missing sampled observation is not an immediate safety violation. A changed digest, changed state ref, or committed-index rollback is a violation.

Profiles declare `lossless` or `sampled` observation mode. A lossless gap is observer-conformance failure. A sampled gap reduces coverage without fabricating divergence.

**Rationale:** Correct replicas can progress at different rates. They cannot apply different command histories at the same position.

### 4. Keep safety separate from conditional liveness

**Choice:** Safety evaluation runs on every accepted observation prefix. Liveness evaluation requires an explicit stabilization precondition and named progress bound.

The precondition binds an available quorum, inactive disruptive faults, admitted lifecycle state, and a virtual progress horizon. Wall-clock delay cannot decide the semantic verdict.

**Rationale:** A partition can legally stop progress. It cannot make divergent committed histories legal.

### 5. Preserve indefinite proposal outcomes

**Choice:** Proposal results are `acknowledged`, `definitely-rejected`, or `indefinite`. Each logical operation has one stable operation identity across retries.

An indefinite result does not prove that the command was absent. Later replica observations determine whether it joined the committed history.

**Rationale:** Timeouts and connection loss can occur after commitment but before acknowledgement.

### 6. Make campaigns bounded and diversity-preserving

**Choice:** Every profile includes a no-fault control. Fault campaigns declare finite fault classes, weights, concurrency, workload length, virtual duration, and terminal rules.

Swarm runs select declared feature and fault subsets from seeded choices. Reports retain the selected subset and unexplored coverage.

Only applied and observed fault stages from `verify-fault-application-outcomes` can support effect claims.

**Rationale:** Random campaigns need replay and coverage facts. Selected faults are not evidence of applied or observed effects.

### 7. Use a functional core and a thin shell

**Choice:** Profile admission, hash transitions, history normalization, safety evaluation, liveness evaluation, and verdict classification are pure deterministic functions.

Guest I/O, SDK calls, VM control, artifact reads, persistence, and report output remain imperative shells.

**Rationale:** The semantic checker must run against fixtures without KVM, processes, files, clocks, or network access.

### 8. Keep runtime evidence typed and bounded

**Choice:** Rust owns hot-path proposal and observation records. A compact review receipt binds profile and build refs, seed, choices, fault outcomes, observation summary, verdicts, bounds, replay result, and non-claims.

Nickel owns the human-authored profile contract and validates compact projected inputs. Rust revalidates all external projections.

**Rationale:** Runtime traces are not practical human-authored configuration. Review profiles need typed contracts.

### 9. Keep consumer evidence external and scoped

**Choice:** Consumers package their own executable, adapter, state-machine semantics, secrets policy, and assertion catalog. ChaosControl records observations but does not grant consumer authority or release eligibility.

Cross-repository inputs use immutable package, schema, and artifact refs. Workspace-relative paths cannot become product behavior.

**Rationale:** A generic harness cannot inherit or replace the consumer's trust and lifecycle boundaries.

## Functional core / imperative shell split

- **Pure core**: profile admission, chain transition, observation validation, safety and liveness evaluation, indefinite-outcome handling, replay comparison, and verdict classification.
- **Shell**: load profiles, start guests, submit commands, read observations, control faults, persist artifacts, invoke replay, and render reports.

## Risks / Trade-offs

- Equal BLAKE3 chains do not prove that the common command sequence was valid for the application.
- A faulty observer can hide or fabricate transitions. The receipt must bind the observer implementation, path, completeness mode, and dropped-event accounting.
- Incorrect liveness preconditions can create false failures. Every liveness verdict remains profile-bound.
- The state space is unbounded in principle. Campaign evidence must retain finite bounds and unexplored coverage.
