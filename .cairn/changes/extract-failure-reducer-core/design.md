## Context

The current minimizer uses a local `ddmin` loop. A closure reruns the VMM for each candidate. The closure also interprets reports and target assertions. This shape is hard to reuse and hard to resume safely.

The reusable core needs an explicit request and response protocol.

## Decisions

### Decision: Publish a caller-driven state machine

**Choice:** The core accepts an ordered source set and policy. Each transition returns either a candidate-evaluation request, a completed result, or a typed failure. The caller later supplies the outcome.

**Rationale:** The core stays pure and does not need callbacks, async runtimes, subprocesses, VMs, or mocks.

### Decision: Preserve deterministic candidate order

**Choice:** Candidate partitioning, complement order, granularity changes, tie breaks, and completion use one versioned deterministic algorithm.

**Rationale:** Equal inputs and outcomes must produce equal candidates and transcripts.

### Decision: Model indeterminate outcomes

**Choice:** Predicate outcomes are `Reproduces`, `DoesNotReproduce`, or `Indeterminate` with a typed reason. Policy selects fail, bounded retry request, or conservative retention.

**Rationale:** Timeouts, blocked environments, and invalid fixtures are not negative predicate evidence.

### Decision: State the minimality claim precisely

**Choice:** Successful completion reports a locally one-minimal result for the evaluated sequence under the declared deterministic predicate transcript. It does not claim globally smallest cardinality.

**Rationale:** Delta debugging cannot prove global optimality from its tested candidates.

### Decision: Bound all work

**Choice:** Callers provide named limits for source items, candidate evaluations, retained transcript entries, and indeterminate retries. Checked counters stop before a limit is crossed.

**Rationale:** A reducer must not turn one failure into unbounded execution or evidence growth.

### Decision: Bind the transcript with BLAKE3

**Choice:** A deterministic transcript binds the algorithm version, source identity, policy, ordered candidate identities, supplied outcomes, and final status with domain-separated BLAKE3.

**Rationale:** Consumers need stable identity for the exact reduction observation.

### Decision: Keep predicate authority in consumers

**Choice:** ChaosControl decides whether a candidate reproduces the exact assertion failure. It must use accepted replay evidence and observed fault effects where required.

**Rationale:** The generic reducer cannot know whether a VM result is authoritative.

## Risks / Trade-offs

- Explicit stepping adds orchestration code but makes resume and testing simpler.
- Indeterminate policy can affect the final retained set and must enter transcript identity.
- A stable algorithm version limits internal optimization unless a new version is explicit.
- Consumers can still provide an unsound predicate. The transcript only records supplied outcomes.
