## Context

The current deterministic block model shares an immutable byte vector and clones only dirty pages. It supports read and write errors, torn writes, corruption, snapshot, and restore. The same module also reads image files and carries VMM-specific fault types.

The shared crate must keep storage transitions deterministic over supplied state and plans.

## Decisions

### Decision: Publish an independent crate in `deterministic-sim`

**Choice:** Add `deterministic-block` as an independent AGPL package. It does not require scheduler, clock, or KVM adapter crates.

**Rationale:** Storage tests can use the device model without the rest of a simulator runtime.

### Decision: Make geometry explicit

**Choice:** `BlockGeometry` names logical block size, copy-on-write page size, total capacity, maximum transfer bytes, and maximum dirty pages. Construction checks divisibility and range invariants.

**Rationale:** Hard-coded sector and page values limit reuse and hide resource policy.

### Decision: Separate planning from mutation

**Choice:** Pure planners check ranges, arithmetic, geometry, fault plans, allocation bounds, and resulting layer actions. A thin in-memory shell applies an accepted plan once.

**Rationale:** Invalid operations must fail before buffer or overlay mutation.

### Decision: Preserve three storage layers

**Choice:** The model uses an immutable shared base, a durable overlay, and a volatile overlay. Reads use documented precedence. Flush moves accepted volatile state into durable state. Restore reconstructs all layers.

**Rationale:** The layers support crash and durability experiments without host filesystem effects.

### Decision: Supply fault choices from outside

**Choice:** Read failure, write failure, torn extent, and corruption position enter as explicit validated operation plans. The block crate does not read entropy or choose a fault schedule.

**Rationale:** The scheduler or fault engine owns choice. The device model owns the resulting storage transition.

### Decision: Keep external I/O outside the model

**Choice:** Consumers provide admitted base bytes or a bounded byte source. File opening, mapping, decompression, artifact lookup, and persistence remain in consumer shells or shared bounded-input mechanisms.

**Rationale:** A storage model does not need filesystem authority.

### Decision: Keep artifact identity outside

**Choice:** The crate exports complete versioned snapshot facts. Artifact packaging, chunk identity, BLAKE3 receipts, retention, and deletion remain with consumer and artifact owners.

**Rationale:** Snapshot state is not an artifact lifecycle mechanism.

## Risks / Trade-offs

- General geometry can add checks to a hot VMM path.
- Overlay representation changes can alter memory use even when bytes match.
- Flush semantics need exact documentation to avoid false durability claims.
- Large snapshot comparison needs bounded fixtures and digest-backed adapter checks.
