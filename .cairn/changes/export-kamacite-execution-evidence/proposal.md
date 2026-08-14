## Why

ChaosControl owns deterministic VM scheduling, fault execution, snapshots, replay, and runtime evidence. Kamacite now provides the portable semantic-operation foundation for cross-runtime execution profiles.

ChaosControl does not yet export its choices, fault outcomes, and replay links through that portable vocabulary. Product simulators and VM campaigns therefore cannot share exact profile identities without merging evidence classes.

## What Changes

- Admit one exact Kamacite deterministic execution profile before an opted-in campaign or replay export.
- Emit Rust-owned choice, fault, effect-log, snapshot, and replay records bound to that profile.
- Preserve attempted, applied, and observed fault states as separate facts.
- Require explicit workload adapter mappings between semantic, runtime, and host effect operations.
- Emit a Kamacite compatibility projection after runtime records pass ChaosControl validation.
- Keep product property receipts separate from ChaosControl run and replay receipts.
- Add a KVM-free export rail and bounded KVM producer rail with explicit blocked status.

## Impact

- **Contracts**: Extend simulator and campaign profiles with optional Kamacite identity bindings.
- **Core**: Pure projection planning, role checks, mapping admission, and deterministic diagnostics.
- **Shell**: Runtime record collection and compatibility export after campaign or replay completion.
- **Evidence**: New BLAKE3 identities for projections while existing snapshot SHA-256 remains unchanged.
- **Testing**: Static fixtures, negative mappings, runtime-record fixtures, and bounded KVM smoke evidence.

## Out of Scope

- Making ChaosControl the canonical operation, profile, or Evidence IR owner.
- Inferring high-level product semantics from packets, block requests, syscalls, or timing.
- Defining or evaluating product invariants.
- Converting missing KVM into a passing result.
- Claiming universal determinism, semantic equivalence, replay completeness, or release eligibility.
