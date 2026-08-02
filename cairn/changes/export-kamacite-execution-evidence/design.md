## Context

ChaosControl already separates Nickel-authored profiles from Rust-owned observations. It records scheduler decisions, faults, checkpoints, snapshots, assertion events, and replay verdicts.

Kamacite owns portable semantic operation identities, effect rows, handlers, replay classes, and the proposed deterministic execution profile. Valence owns evidence identity and linkage.

This change adds one producer adapter. It does not move runtime behavior into Kamacite or evidence semantics into ChaosControl.

## Decisions

### Decision: Admit an exact Kamacite profile binding

**Choice:** An opted-in campaign or replay export binds one canonical Kamacite execution profile identity and its compatibility projection identity.

The binding also names the application-host binding, effect summary, handler cohort, scheduler policy, fault policy, and required non-claims.

Only pre-run artifacts can enter profile identity. A profile cannot depend on a runtime record or projection that already depends on that profile.

**Rationale:** Runtime records must refer to the exact reviewed portable composition without creating an identity cycle.

### Decision: Extend Nickel profiles only with reviewed input identities

**Choice:** Nickel-authored simulator and campaign profiles carry optional Kamacite identity bindings, projection references, adapter mappings, and export policy.

Rust still owns observed choices, faults, traces, snapshots, outcomes, and receipts.

**Rationale:** Pre-run contracts remain human-reviewed while runtime facts remain execution records.

### Decision: Rust emits portable record projections

**Choice:** Rust-owned records expose fields required for Kamacite `ChoiceTrace`, `FaultApplicationReceipt`, and effect-log projections.

The export binds exact profile, operation, handler, step, actor, generation, logical time, causal parent, input, output, fault, snapshot, and replay identities.

**Rationale:** Nickel cannot author high-volume observations after execution.

### Decision: Preserve fault state transitions

**Choice:** Every exported fault records scheduled, attempted, applied, and observed states with distinct typed fields and reasons.

A scheduled or attempted fault cannot satisfy applied or observed evidence. Unsupported targets fail before execution when possible.

**Rationale:** Fault schedule intent and runtime effect are different evidence classes.

### Decision: Keep effect strata explicit

**Choice:** Workload adapters map exact product semantic operations to declared runtime or host operations.

ChaosControl never infers a high-level operation from packet, disk, process, clock, interrupt, or syscall proximity. Name equality is not a mapping.

**Rationale:** VM observations are lower-level facts and cannot define product semantics.

### Decision: Replay binds all required parents

**Choice:** A replay export binds the execution profile, choice trace, effect-log segments, fault receipts, workload artifact, VM cohort, snapshot, and replay verdict.

Existing protocol-required snapshot SHA-256 remains unchanged. New ChaosControl-owned profile and projection identities use BLAKE3.

**Rationale:** A replay verdict without exact choice, fault, artifact, and snapshot parents is incomplete linkage evidence.

### Decision: Product properties remain paired external receipts

**Choice:** ChaosControl can reference a product-owned property receipt through exact subject, trace, and observation identities.

It does not create that receipt, interpret its invariant, or merge its role with run or replay evidence.

**Rationale:** Products retain their semantic oracle and property authority.

### Decision: Export follows ChaosControl validation

**Choice:** The pure core validates loaded runtime facts and derives a deterministic projection plan. The shell writes the compatibility projection only after validation succeeds.

Kamacite remains the canonical Preserves owner. Valence remains the Evidence IR linkage owner.

**Rationale:** ChaosControl must not publish successful portable linkage from malformed or stale runtime records.

### Decision: Keep fast and KVM rails separate

**Choice:** A default KVM-free rail validates profile bindings, mappings, projection shape, fixtures, and non-claims.

A bounded KVM rail produces fresh runtime records. Missing KVM, unsupported architecture, or denied device access produces `blocked`.

**Rationale:** Ordinary checks stay fast while runtime evidence still requires actual execution.

## Test Design

Positive fixtures cover static projection, complete choices, unapplied faults, applied faults, observed faults, snapshot-backed replay, product receipt references, and explicit effect lowering.

Negative fixtures cover stale profiles, identity cycles, unknown operations, inferred semantic mappings, collapsed fault states, missing causal parents, tampered snapshots, replay-parent gaps, role promotion, and missing KVM.

Property tests cover input-order independence, state-transition validity, projection identity, complete parent binding, and deterministic diagnostics.

## Risks / Trade-offs

- Portable exports can increase trace volume. Profile bounds and linked segments limit output.
- Workload mappings require product effort. Explicit mappings prevent stronger false claims.
- Kamacite schema changes can block exports. Exact versioned cohorts make drift visible.
- KVM runs remain host-dependent. Blocked status preserves the distinction from passing static checks.

## Claim Boundary

A passing export proves that ChaosControl records match one admitted portable projection shape. It does not prove product semantics or universal replay correctness.
