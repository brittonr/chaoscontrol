# Ebpf Trace Evidence Specification

## Purpose

Defines the `ebpf-trace-evidence` capability.

## Requirements

### Requirement: eBPF trace captures use typed bounded profiles

r[ebpf_trace_evidence.profile] ChaosControl MUST define a versioned capture profile binding exact event schema, BPF object/source, loader, kernel/BTF/tracepoint layouts, target run/process and filter, VMM/vCPU/CPU producer topology, enabled events, ordering mode, named ring/queue/poll/event/artifact bounds, retention, and non-claims.

#### Scenario: Complete capture profile is admitted

- GIVEN a profile binds one exact cohort, target, topology, ordering mode, finite bounds, and non-claims
- WHEN profile validation runs
- THEN it MAY proceed to runtime cohort admission
- AND every trace artifact MUST retain the profile identity.

#### Scenario: Capture profile is incomplete or unbounded

- GIVEN a profile omits an artifact/cohort/target/topology identity, permits undeclared events, lacks finite bounds, or omits evidence scope
- WHEN profile validation runs
- THEN it MUST fail before BPF loading.

### Requirement: Runtime layout and artifact admission is exact

r[ebpf_trace_evidence.admission] Evidence-eligible capture MUST verify the exact BPF object and loader identities, running kernel release/architecture/BTF identity, and canonical required tracepoint-format signatures against the compiled event projection before attach; fallback type-stub compilation alone MUST NOT establish runtime compatibility.

#### Scenario: Exact runtime cohort matches

- GIVEN object, loader, kernel, BTF, and every required tracepoint field signature match the profile and build metadata
- WHEN runtime admission runs
- THEN the collector MAY load and attach for the declared evidence mode.

#### Scenario: Fallback or runtime layout is unverified

- GIVEN the object used fallback type stubs, BTF is absent or changed, a tracepoint is missing, a field signature differs, or an artifact identity drifts
- WHEN runtime admission runs
- THEN accepted evidence capture MUST be blocked
- AND any explicit debug capture MUST retain unsupported status.

### Requirement: Capture accounting detects loss and rejection

r[ebpf_trace_evidence.accounting] The producer MUST expose bounded per-source eligible-attempt, submitted-record, and ring-reservation-drop counters, userspace MUST account received, malformed, unknown, parse-failed, over-bound, callback/lock-failed, poll-failed, and final-drain outcomes, and pure epoch completion MUST reconcile all required counters with checked arithmetic.

#### Scenario: Complete zero-loss capture reconciles

- GIVEN producer attempts equal submissions, reservation drops are zero, userspace accepted every submitted record, and final drain succeeds
- WHEN accounting reconciliation runs
- THEN the epoch MAY be classified complete for its exact profile and target.

#### Scenario: Any loss or accounting uncertainty exists

- GIVEN a reservation fails, a sequence gap or malformed/unknown record occurs, userspace drops a record, a counter overflows or mismatches, or required final accounting is unavailable
- WHEN reconciliation runs
- THEN the epoch MUST be partial, failed, or unsupported
- AND it MUST NOT satisfy complete trace comparison.

### Requirement: Ordering evidence preserves source limits

r[ebpf_trace_evidence.ordering] Events MUST retain CPU/source identity, source-local sequence, and delivery provenance separately; exact event-order comparison MUST be limited to an admitted single-producer topology with continuous sequence and zero loss, while multi-producer captures MUST use declared partial-order or bounded aggregate comparison and MUST NOT derive semantic total order from host timestamps or callback order.

#### Scenario: Single-producer exact mode is eligible

- GIVEN the target topology and affinity constrain capture to the declared single producer, sequence is continuous, and accounting is complete
- WHEN ordering admission runs
- THEN source-local event order MAY be used for exact comparison.

#### Scenario: Multi-producer arrival order differs

- GIVEN equivalent source-local streams arrive in another cross-CPU interleaving
- WHEN multi-producer comparison runs
- THEN declared partial-order or aggregate facts MAY match
- AND timestamp or capture-index sorting MUST NOT be reported as exact total-order equivalence.

### Requirement: Parsing and comparison are pure and fail closed

r[ebpf_trace_evidence.comparison] ChaosControl MUST validate raw record size, version, discriminant, payload, sequence, completeness, cohort compatibility, and ordering-mode eligibility and MUST canonicalize and compare traces through pure deterministic functions over in-memory inputs; unknown events MUST NOT masquerade as known events.

#### Scenario: Compatible complete traces match

- GIVEN two complete traces share the exact compatible cohort/profile and satisfy the selected ordering mode
- WHEN pure comparison runs
- THEN equal canonical observations MAY produce a bounded match receipt.

#### Scenario: Traces are malformed, incomplete, or incompatible

- GIVEN either trace has an unknown event, malformed payload, accounting failure, different cohort/profile, or ineligible ordering mode
- WHEN comparison runs
- THEN it MUST return a typed non-pass result rather than compare raw vectors or coerce the input.

### Requirement: Target and collector lifecycles are identity bound

r[ebpf_trace_evidence.lifecycle] Capture MUST bind an exact run and stable process-lifetime identity beyond numeric PID, MUST detect target exit/reuse or executable/profile drift, and MUST explicitly own and classify open, load, map-update, attach, poll, final-drain, detach, unpin, and cleanup states without relying on leaked resources or process exit as accepted cleanup evidence.

#### Scenario: Exact target completes and cleans up

- GIVEN the target identity remains stable and capture reaches its declared boundary
- WHEN shutdown runs
- THEN final accounting, detach, unpin where applicable, and cleanup MUST receive explicit terminal outcomes
- AND accepted evidence MUST bind those outcomes.

#### Scenario: Target or cleanup drifts

- GIVEN the PID is reused, target exits or execs unexpectedly, a partial attach occurs, or detach/unpin/cleanup fails
- WHEN lifecycle classification runs
- THEN the epoch MUST not be a complete accepted capture
- AND unrelated or foreign kernel resources MUST NOT be removed.

### Requirement: Trace evidence is canonical, linked, and narrow

r[ebpf_trace_evidence.evidence] ChaosControl MUST emit domain-separated BLAKE3 trace manifests and comparison receipts binding exact profile, artifacts, runtime cohort, target, topology, accounting, ordering mode, bounded event or aggregate refs, divergence, terminal state, and cleanup while excluding credentials, private host paths, raw command lines, unbounded logs, and overclaims.

#### Scenario: Complete comparison receipt is emitted

- GIVEN compatible complete traces are compared in an eligible mode
- WHEN receipt projection runs
- THEN it MAY report bounded KVM trace-observation match or divergence for that exact cohort
- AND it MUST distinguish artifact, accounting, ordering, target, and cleanup facts.

#### Scenario: Matching trace is promoted to a stronger proof

- GIVEN a consumer presents matching trace evidence as VM determinism, replay correctness, eBPF safety, kernel correctness, security proof, physical readiness, or release eligibility
- WHEN evidence-role validation runs
- THEN the scope promotion MUST be rejected.

### Requirement: eBPF trace hardening has positive and negative rails

r[ebpf_trace_evidence.verification] ChaosControl MUST provide cheap positive and negative pure/schema/layout/source-guard conformance plus a separate privileged KVM/eBPF rail covering admission, loading, attachment, capture, loss, ordering, target lifecycle, final drain, detach, cleanup, evidence, and blocked prerequisites.

#### Scenario: Supported privileged cohort passes

- GIVEN the exact supported KVM/eBPF/BTF cohort and capabilities are available
- WHEN the privileged rail runs
- THEN zero-loss capture, deliberate divergence, loss detection, target exit, detach, cleanup, and evidence-scope cases MUST produce their expected bounded classes.

#### Scenario: Privileged prerequisites are missing

- GIVEN required capabilities, KVM, BTF, tracepoints, or the pinned loader are unavailable
- WHEN the privileged rail runs
- THEN it MUST report blocked with remediation
- AND cheap or debug success MUST NOT count as accepted capture evidence.
