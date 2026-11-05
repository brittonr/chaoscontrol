## Context

The explorer already receives structured assertion state after each branch and hashes it into a coverage region. It also retains snapshots for replay parents and marker-driven frontier entries. What it lacks is an event boundary: by the time a report is assembled, the exact guest state at the assertion observation is gone.

The assertion path is a hypercall or port-I/O exit. The controller handles that exit before the guest executes another instruction. That boundary is the only deterministic point at which a hit can be both identified and captured without reconstructing an earlier state.

## Decisions

### Decision: Observe at the post-response, pre-progress boundary

**Choice:** the VMM emits a bounded hit observation after the assertion hypercall has been fully handled and before the next guest instruction executes. The observation carries the admitted identity, kind, verdict, guest/process owner, virtual tick, and the observed VM/vCPU. A capture is taken only at that boundary.

**Rationale:** a later report-derived hit cannot reproduce guest state. An asynchronous host sampling point is not deterministic. The exit boundary is already part of the exact execution evidence.

### Decision: Selection is a pure, seeded policy

**Choice:** the explorer owns a pure policy that receives admissible hit facts, prior hit counts, frontier state, and named limits. It returns a deterministic ordered decision for each hit: capture or skip, with a stable reason. All randomness derives from the campaign seed.

**Rationale:** capture is expensive and unbounded checkpoints would displace coverage-guided branches. A pure policy is testable without KVM and remains replayable.

### Decision: Checkpoints are frontier proposals, not assertions of importance

**Choice:** a captured hit becomes a branch proposal with assertion provenance. It must pass the same deduplication, parent validation, and restore checks as a marker or fault proposal.

**Rationale:** one source of frontier truth keeps scoring and replay consistent. A hit is evidence that a state was reached, not proof that branching there will find a bug.

### Decision: Liveness classification stays with the existing change

**Choice:** this change never marks a `sometimes` or `reachable` assertion as a bug and never changes its verdict. `evaluate-sometimes-assertions-campaign-wide` owns campaign-wide liveness and safety-only bug extraction.

**Rationale:** mixing amplification with classification would reintroduce the noise that the classification change removes.

### Decision: Capture failure is local and typed

**Choice:** snapshot allocation, encoding, storage, or validation failure marks the selected checkpoint as not captured with its exact reason. The campaign continues unless an operator explicitly makes checkpoint capture fatal. No partial snapshot becomes a frontier parent.

**Rationale:** a full campaign must not become unusable because one optional amplification candidate could not be retained.

## Risks / Trade-offs

- Capturing every rare hit can consume disk and scheduling time; per-run and per-campaign limits, plus deterministic skip reasons, keep the cost visible.
- Assertion-heavy workloads can produce many duplicate hits at one logical state; identity plus tick plus canonical state deduplication prevents unbounded frontier growth.
- A hit can be frequent yet uninteresting; prior hit rarity and frontier novelty must be combined, with the score fields retained for review.
- Snapshot capture at a hypercall boundary must be verified against exact replay; without a KVM fixture this change cannot establish that boundary.
