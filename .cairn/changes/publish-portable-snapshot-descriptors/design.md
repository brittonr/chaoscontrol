## Context

ChaosControl snapshots preserve complete state for one exact x86 KVM profile. Snapshot payloads and replay-parent references currently serve in-repository replay and dogfood workflows.

A consumer must not infer public compatibility from a host path or import an internal Redb index. The producer needs a stable descriptor that exposes only bounded state, closure, and compatibility facts.

## Decisions

### Decision: Publish a descriptor, not a new payload

**Choice:** Define `chaoscontrol-snapshot-descriptor-v1` around the existing snapshot payload and chunk formats. The descriptor binds payload identities, codecs, lengths, closure, and restore requirements.

This change does not rewrite snapshot bytes or create another compression and archive format.

**Rationale:** Payload fidelity is already implemented and tested. The missing surface is a stable product-neutral description and preflight boundary.

### Decision: Give the descriptor a Rust-owned canonical identity

**Choice:** Rust DTOs own fields and validation. A closed canonical hash-material projection uses domain-separated BLAKE3 framing over versioned, length-delimited fields and deterministic ordered inventories.

JSON is a public machine projection and Nickel review surface. JSON byte formatting is not descriptor identity. An optional Kamacite adapter may project the admitted DTO into Preserves.

**Rationale:** Runtime records remain Rust-owned while canonical identity does not depend on unstable JSON formatting.

### Decision: Bind a complete compatibility cohort

**Choice:** The descriptor includes exact profile and state-schema IDs, architecture, KVM operations, sorted MSR inventory, CPU topology, memory shape, stable device identities, backend kinds, deterministic state-owner inventory, guest artifact refs, scheduler profile, time profile, entropy profile, and payload closure.

Unknown, missing, duplicate, or extra required components fail closed.

**Rationale:** A snapshot is restorable only relative to every behavior-relevant state owner and destination capability.

### Decision: Separate content from location

**Choice:** Canonical descriptors contain tagged content digests, byte lengths, chunk order, logical payload identity, and closure roles. Store names, paths, Redb keys, Iroh tickets, mirror URLs, and provider handles remain detached locator observations.

Existing interoperability digests keep their required algorithm tag. ChaosControl-owned descriptor identity uses BLAKE3.

**Rationale:** Location can change without changing snapshot meaning. A locator cannot prove bytes or availability.

### Decision: Keep preflight pure and restore observational

**Choice:** Pure preflight compares descriptor facts with supplied destination observations and returns an ordered restore plan or blockers. It performs no KVM, filesystem, memory, device, or network effects.

The shell records materialization, mutation-start, phase failures, poison, completion, and bounded continuation observations in a detached restore receipt.

**Rationale:** Descriptor compatibility does not prove successful restore. Failure after mutation starts must preserve existing poison semantics.

### Decision: Keep consumer authority outside the descriptor

**Choice:** Descriptor validity grants no read, transfer, retention, restore, execution, branch, or release authority. Consumers must separately admit artifact access, destination resources, runtime policy, and execution authority.

**Rationale:** Portable metadata must not become a bearer capability.

### Decision: Publish one bounded consumer fixture

**Choice:** Add a fixture that lets Molten validate descriptor identity, complete closure, and exact-profile compatibility. The fixture does not depend on Molten crates and does not encode world-commit semantics.

**Rationale:** A real external-shaped consumer proves the boundary without transferring ownership.

## Rollout

1. Inventory the exact snapshot state and current artifact-reference fields.
2. Define descriptor DTOs, bounds, canonical hash material, and JSON projection.
3. Generate the Nickel review contract and fixture corpus.
4. Emit descriptors for monolithic and chunked snapshots.
5. Add pure destination preflight and detached restore receipts.
6. Publish the consumer fixture and immutable contract documentation.

## Risks / Trade-offs

- The exact cohort is verbose. Completeness is more important than a small descriptor.
- Internal snapshot fields can change. Version the descriptor and state schema independently.
- Existing SHA-256 artifact names can coexist with BLAKE3 descriptor identity. Algorithm tags prevent reinterpretation.
- A descriptor can outlive its payload. Availability and retention remain separate observations.
- Consumer demand can tempt generic portability claims. Keep the exact profile and topology limits explicit.
