# Design: Projected role-protocol fault campaigns

## Context

Trellis and Choregraph own protocol semantics and projection. Lattice owns protocol runtime state, persistence, adapters, retries, and observations. ChaosControl owns deterministic execution and fault evidence for exact supplied workloads.

The campaign must detect protocol-state violations. Delivery or process output alone cannot distinguish a safe blocked state from a false success.

## Success Contract

The accepted result is one bounded campaign family with frozen producer and runtime adapters, independent expected outcomes, protocol assertions, explicit fault schedules, complete observation accounting, and replayable evidence.

A bounded unsupported result is valid when KVM, guest, transport, or artifact prerequisites are absent. Unsupported is never a pass.

## Approach Registry

| Family | Mechanism | State |
| --- | --- | --- |
| Protocol-aware deterministic campaign | Frozen artifacts, independent outcomes, exact assertions, selected faults | selected |
| Generic packet campaign | Check only send and receive counts | rejected |
| Runtime self-oracle | Ask Lattice whether its own result is correct | rejected |
| Compiler proof promotion | Treat projectability as runtime fault safety | rejected |
| Physical fleet campaign | Run against arbitrary remote hosts | deferred |

## Producer and Runtime Cohorts

The campaign consumes two immutable cohorts.

The Choregraph cohort contains source revision, global artifact schema, local-set schema, projection receipt schema, canonical artifact identities, and selected proof references.

The Lattice cohort contains runtime source revision, session schema, envelope schema, persistence schema, outcome classes, recovery actions, adapter profile, and selected fixtures.

Each adapter is versioned and narrow. Field-name similarity does not permit compatibility. Working-tree sibling paths are development inputs only.

## Campaign Profile

Nickel owns a human-authored profile containing:

- campaign schema and identity;
- exact producer and runtime cohorts;
- global, local-set, placement, and session fixtures;
- guest, kernel, initrd, harness, and workload identities;
- roles and role-instance placement;
- selected transfer and choice cases;
- independent expected-outcome fixture identity;
- fault classes, activation points, durations, and heal conditions;
- required assertions and observation producers;
- execution, message, queue, event, snapshot, replay, artifact, and time bounds; and
- evidence scope and non-claims.

Rust owns runtime events, packet observations, checkpoints, assertion records, replay traces, and outcomes.

## Independent Expected Outcomes

Each selected case has a frozen expected-outcome fixture reviewed separately from runtime execution.

The fixture identifies initial role cursors and owners, input action or fault, allowed terminal classes, forbidden state changes, required assertions, and expected recovery eligibility.

Campaign execution must not call the Lattice runtime under test to generate its only expectation. A selected Trellis or Choregraph reference reducer can provide additional comparison evidence, but it does not replace manually reviewed fault expectations.

Negative fixtures alter session, role, peer, step, label, payload tag, value, attempt, artifact, or expected outcome. The campaign must detect every mismatch.

## Protocol Assertions

The first campaign catalog includes stable assertions for these facts:

- a wrong session never advances a cursor;
- a wrong source or destination role never advances a cursor;
- a wrong local artifact or step never advances a cursor;
- one message identity cannot commit twice;
- reordering cannot skip an expected action;
- a stale or unknown label cannot select a branch;
- a former owner cannot dispatch a committed value;
- replay emits no transport effect;
- unknown dispatch remains unknown until explicit recovery; and
- terminal session records reject later mutation.

Assertion identities bind namespace, logical key, kind, message, source site, guest, and category under existing ChaosControl rules.

## Fault Matrix

The deterministic network fabric supplies loss, delay, duplication, reordering, corruption, partition, bandwidth limits, and heal when supported.

The process and VM shell supplies selected role termination and restart points.

Fault points include:

- before prepared-attempt persistence;
- after persistence and before dispatch;
- after dispatch and before receiver observation;
- after receiver observation and before commit publication;
- before and after choice-label dispatch;
- during partition and after heal; and
- before replay or explicit recovery.

Every selected fault retains selected, applicable, applied, observed, healed, failed, and indeterminate stages.

## Outcome Classification

The pure classifier returns one of these bounded classes:

- expected completion;
- expected block;
- explicit unknown outcome;
- expected terminal failure;
- assertion violation;
- protocol mismatch;
- transport outcome;
- guest or runtime failure;
- partial observation;
- unsupported; or
- indeterminate.

A missing message alone does not establish a safe block. Complete classification requires the selected runtime state and observation accounting.

## Observation Accounting

Required event producers retain generation, source-local sequence, event class, bounds, loss counters, final drain, and cleanup observations.

Any required sequence gap, overflow, truncation, malformed event, unknown event, queue loss, parse failure, missing terminal accounting, or failed cleanup prevents complete classification.

Timestamps across producers do not create a semantic total order. Protocol message and persisted step identities supply ordering evidence within their defined scope.

## Replay and Evidence

A receipt binds:

- producer and runtime cohorts;
- protocol, placement, session, and oracle fixtures;
- guest and harness artifacts;
- fault schedule and assertion catalog;
- observations and accounting;
- initial and terminal state identities;
- snapshot and replay references;
- outcome, blockers, and non-claims; and
- one domain-separated BLAKE3 receipt identity.

Snapshot-backed replay can establish reproduction of one exact selected campaign outcome. It does not prove all schedules or deployments.

## Functional Core and Imperative Shell

Pure cores own profile admission, adapter checking, case expansion, expected-outcome comparison, assertion evaluation, fault applicability, observation accounting, classification, and receipt preimages.

Shells own Nickel export, files, KVM, guests, simulated devices, process death, clocks, persistence, snapshots, replay execution, and output publication.

## Validation Strategy

Positive cases cover fault-free transfer, fault-free choice, expected blocking, explicit unknown outcome, heal recovery, complete observation, and snapshot-backed replay.

Negative cases cover stale producer or runtime cohort, tautological oracle, wrong session, role, peer, step, label, value, duplicate commit, reordered skip, former-owner use, replay dispatch, false success, incomplete observation, missing KVM, and evidence overclaim.

The cheap rail exercises pure decisions and in-process simulation. The KVM rail runs a separate exact guest cohort and cannot be replaced by compile-only or dry-run evidence.

## Adversarial Audit

The audit must try to make a forbidden state advance while keeping process exit and packet counts normal. It must also try to turn missing evidence into success.

Any reproducible assertion violation, oracle tautology, incomplete accounting promotion, replay dispatch, or stale-cohort acceptance blocks archive.

## Risks and Trade-offs

- A manually reviewed oracle covers only selected cases.
- KVM campaigns are slower than pure and in-process checks.
- Runtime schema drift can invalidate the frozen adapter.
- A fault can create incomplete evidence rather than a decisive outcome.
- One deterministic schedule does not represent all schedules.

## Non-Claims

This design does not prove compiler correctness, runtime correctness, external-role correctness, universal determinism, universal deadlock freedom, exactly-once delivery, transport confidentiality, physical network behavior, production availability, readiness, or release eligibility.
