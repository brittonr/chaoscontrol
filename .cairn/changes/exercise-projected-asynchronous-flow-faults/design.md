## Context

ChaosControl has deterministic network and process faults, snapshots, replay, assertion catalogs, and typed evidence. The active protocol-observation cohort change provides a generic mechanism for opaque participant observations and completeness accounting.

The existing role-protocol campaign assumes finite expected peer actions. The asynchronous-flow pilot has another oracle boundary. Reordering and duplication are expected for an ACI reducer, while missing closure or lost observations prevent a completed result.

## Success Contract

The accepted result is one bounded asynchronous-flow campaign family with frozen producer, proof, runtime, and observation cohorts. Independent fixtures define expected outcomes. Pure oracles classify complete and incomplete executions.

A bounded unsupported or indeterminate result is valid evidence when prerequisites are missing. Neither status is a pass.

## Approach Registry

| Family | Mechanism | State |
| --- | --- | --- |
| Protocol-aware flow campaign | Exact cohorts, independent oracle, selected faults, complete accounting | selected |
| Packet-count campaign | Compare only send and receive totals | rejected |
| Runtime self-oracle | Let Lattice report its own correctness | rejected |
| Compiler proof promotion | Treat ACI proof as runtime convergence under faults | rejected |
| Arbitrary physical fleet | Run unpinned hosts and transports | deferred |

## Decisions

### Decision: consume frozen cohorts

**Choice:** The campaign consumes immutable Choregraph global and local flows, Trellis law evidence, Lattice runtime records, and protocol-observation contracts.

Each adapter binds repository revision, schemas, artifacts, proof rows, assumptions, fixtures, and source-manifest identities. Similar fields and working-tree paths do not establish compatibility.

**Rationale:** Fault evidence must identify the exact semantic and runtime subjects.

### Decision: keep flow meaning in consumer oracles

**Choice:** ChaosControl owns profile admission, scheduling, observation assembly, fault stages, replay, and evidence. The selected flow oracle owns ACI result comparison, prefix relation, closure requirements, uncertainty rules, and effect-release expectations.

The oracle does not call the Lattice runtime under test to derive its only expected value.

**Rationale:** A runtime and its self-report can share one defect.

### Decision: start with one set-union campaign

**Choice:** Worker roles emit bounded canonical item sets. The reducer consumes an unordered duplicate-bearing edge and computes canonical set union.

The complete oracle compares the final result with an independent union over the admitted logical item cohort. It also validates exact closure and observation completeness.

**Rationale:** This pilot tests the selected algebraic and stream boundaries with a reviewable value domain.

### Decision: classify incomplete prefixes separately

**Choice:** If required items, producer sequences, closure markers, final drains, or cleanup observations are missing, the result is incomplete or indeterminate.

For a valid observed prefix, the oracle can validate that the current result is below the independent complete result. It cannot promote that prefix to completion.

**Rationale:** Monotone progress is useful evidence, but it is not completion.

### Decision: assert effect-release safety

**Choice:** Stable assertions reject a protected effect before full closure, selected window closure, or exact prefix-safety evidence.

The campaign also rejects any result where monotonicity alone appears as execution authority.

**Rationale:** A mathematically valid prefix can still cause an external action too early.

### Decision: exercise semantic fault classes

**Choice:** The first matrix includes deterministic duplication, reordering, delay, loss, partition, heal, role termination, and restart.

Fault activation points include before item persistence, after persistence, before dispatch, after possible dispatch, before observation, before closure, after closure, before effect release, and before outcome commit.

Every fault retains selected, applicable, rejected, applied, application-failed, observed, healed, and indeterminate facts when relevant.

**Rationale:** Fault selection alone does not prove runtime impact.

### Decision: test assumptions as assumptions

**Choice:** A Choregraph nondeterminism assumption selects an expected observational relation and exact campaign profile. The receipt reports whether the selected schedules satisfied that relation within declared bounds.

A passing campaign does not convert the assumption into a theorem.

**Rationale:** Testing can challenge an assumption without changing its evidence class.

### Decision: use protocol-observation cohorts

**Choice:** Participant observations bind producer generation, source sequence, logical boundary, edge, operator, item, result, closure, window, attempt, outcome, and loss accounting through the local generic cohort mechanism.

Sequence gaps, overflow, truncation, unknown records, conflicting values, failed final drains, or failed cleanup prevent complete classification.

**Rationale:** No assertion can compensate for missing required observations.

### Decision: provide cheap and KVM rails

**Choice:** The cheap rail covers profile admission, adapters, case expansion, independent oracles, fault plans, classifiers, and in-process simulation.

A separate KVM rail runs one exact guest cohort and retains a parent snapshot for at least one selected outcome. Unsupported KVM remains explicit.

**Rationale:** Portable logic tests and actual guest replay provide different evidence.

### Decision: keep replay bounded

**Choice:** Snapshot-backed replay validates exact artifacts, schedule, parent snapshot, observations, oracle result, and terminal classification.

Replay of one outcome does not prove all schedules or deployments.

**Rationale:** Reproduction is valuable only within its exact cohort.

## Functional Core and Imperative Shell

Pure cores own profile admission, adapter compatibility, case expansion, fault applicability, expected results, oracle evaluation, observation completeness, assertion evaluation, classification, and receipt preimages.

Shells own Nickel export, files, KVM, guests, simulated devices, process control, snapshots, replay execution, clocks, and artifact publication.

## Validation Strategy

Positive cases cover fault-free union, reordered union, duplicated union, delayed completion, valid incomplete prefixes, partition and heal, explicit uncertainty, complete closure, and snapshot-backed replay.

Negative cases cover stale cohorts, wrong laws, non-canonical sets, false self-oracles, missing items, missing closure, forged closure, observation gaps, duplicate-sensitive counting, order-sensitive concatenation, early protected effects, failed cleanup, and overclaims.

## Adversarial Audit

The audit must try to:

- preserve normal packet counts while corrupting semantic item identity;
- omit one observation and still produce pass;
- turn a valid prefix into completed output;
- release a protected effect before closure;
- cite a law for another reducer or domain;
- use Lattice output as the independent oracle;
- turn an unobserved fault into an applied semantic fault;
- erase a nondeterminism assumption; or
- report KVM unsupported as pass.

Any successful counterexample blocks archive.

## Risks and Trade-offs

- KVM campaigns have higher cost than pure and in-process tests.
- Detailed observation cohorts increase evidence size.
- A selected schedule can miss another fault interaction.
- Independent fixtures can contain their own defects.
- Frozen producer and runtime adapters can become stale often.

## Non-Claims

This design does not prove compiler correctness, proof-system soundness, runtime correctness, all-schedule convergence, transport delivery, exactly-once effects, fault-time liveness, external-role correctness, physical network behavior, production readiness, or release eligibility.
