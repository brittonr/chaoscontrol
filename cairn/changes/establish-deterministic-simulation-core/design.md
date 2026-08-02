## Context

ChaosControl's VMM entropy device uses seeded ChaCha20 and supports snapshots. Its schedulers and fault schedules use deterministic state, but VMM progress and runtime effects remain product-specific. The evidence crate also contains a small round-robin simulator with a separate xorshift generator and saturating clock.

Aspen already defines deterministic scheduler, entropy, time, and simulation abstractions. A neutral repository must contain only the common lower-level mechanism.

## Decisions

### Decision: Require an Aspen seam comparison

**Choice:** Compare state, ordering, snapshot, error, and policy semantics in ChaosControl and Aspen before repository creation. Publish only the common product-neutral subset.

**Rationale:** A third competing simulator model would increase drift instead of reuse.

### Decision: Keep one pure core

**Choice:** The core supports `no_std` plus `alloc`. It consumes typed configuration and supplied progress facts. It performs no clock reads, process work, file I/O, network I/O, KVM calls, or output.

**Rationale:** Determinism claims require explicit inputs and testable transitions.

### Decision: Version virtual time

**Choice:** A virtual clock has an algorithm version, current tick, named quantum or explicit delta policy, and checked advance. Overflow returns a typed failure.

**Rationale:** Saturation can hide divergence and create repeated timestamps.

### Decision: Version entropy streams

**Choice:** Use ChaCha20 with explicit algorithm version, seed material, domain, stream label, byte position, and snapshot state. Zero seeds are ordinary input and do not trigger an undocumented replacement.

**Rationale:** Stream separation and exact resume need complete state.

### Decision: Schedule from supplied stable facts

**Choice:** The scheduler receives the ordered runnable set, stable progress facts, and seeded choice state. It returns one decision and next state. Host wall time and signal arrival are never schedule inputs.

**Rationale:** Consumers can supply KVM or runtime progress without transferring those mechanisms to the shared crate.

### Decision: Make events and choices generic

**Choice:** `Scheduled<Event>` binds event identity, logical tick, deterministic order key, and payload. Recorded choices bind domain identity, option count, selected index, and optional override provenance.

**Rationale:** Faults, packets, tasks, and workload choices can share ordering mechanics without sharing meaning.

### Decision: Snapshot every replay-relevant field

**Choice:** Snapshots include algorithm versions, clock, entropy streams, scheduler state, pending events, recorded choices, counters, and declared limits. Pure preflight rejects incompatible or incomplete snapshots before reconstruction.

**Rationale:** Partial state cannot resume at the same deterministic boundary.

### Decision: Keep consumer policy local

**Choice:** ChaosControl retains KVM progress, VMM scheduling policy, fault application, device adapters, snapshot artifacts, and evidence. Aspen retains Molten runtime policy and distributed semantics.

**Rationale:** The shared core supplies mechanisms, not product claims.

## Risks / Trade-offs

- Aspen and ChaosControl can lack a useful common semantic subset.
- Stable algorithms constrain optimization and require explicit new versions.
- A generic scheduler can become vague if progress facts are not precise.
- Snapshot compatibility adds maintenance cost across repository releases.
