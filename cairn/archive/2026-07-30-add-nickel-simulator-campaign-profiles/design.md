## Context

ChaosControl's current Nickel evidence contracts establish the right ownership rule: people author bounded configuration and review receipts in Nickel, while Rust emits high-volume runtime facts. The gap is that the rule covers one VM run JSON shape but not the in-process simulator or multi-seed campaign inputs, and the existing run contract does not encode many domain relationships.

The design adds an authoring layer, not a second simulation semantics implementation.

## Decisions

### 1. Register every configuration family by authority

**Choice:** Extend `contracts/evidence/registry.ncl` with separate entries for VM run profiles, in-process simulator profiles, campaign profiles, and finite fault-schedule descriptors. Each entry names source, projection, Rust consumer, fixtures, validation commands, freshness, and non-claims.

Runtime campaign progress/reports, simulator observations/receipts, checkpoints, traces, assertion events, fault outcomes, and bug records remain `rust-derived`.

**Rationale:** A clear provider/consumer edge prevents configuration contracts from taking ownership of observed facts.

### 2. Use one hardened configuration prelude

**Choice:** Shared contracts provide exact schema literals, non-empty identifiers, integer classes, named bounds, closed enums, BLAKE3 identities, existing protocol-required digest formats, typed path/reference classes, unique collections, and deterministic validator errors.

**Rationale:** The current contracts repeat broad predicates and permit family drift.

### 3. Model simulator profiles at the existing Rust boundary

**Choice:** A simulator profile mirrors the public authoring subset of `SimulatorConfig`: workload identity, scheduler policy, virtual clock, RNG policy, simulated network/disk profiles, finite schedule reference, artifact map, seed, and bounded claim scope. It requires the currently supported scheduler/RNG/profile vocabulary, positive step/quantum values, non-empty artifact bindings, and exact scope non-claims.

Existing SHA-256-prefixed fields remain SHA-256 where the current Rust receipt contract requires that interoperability shape. New profile/source/projection identities use BLAKE3.

**Rationale:** The contract should prevent malformed intent without silently changing an existing receipt format.

### 4. Campaign profiles cover reusable intent, not runtime progress

**Choice:** A campaign profile covers seed selection, VM/vCPU topology, artifact references, scheduling strategy, exploration mode, branch/round/frontier/quantum/bootstrap limits, worker plan, mutation and havoc ranges, coverage mode, scenario reference, logging/metrics policy, and output-layout policy.

Whole-profile validation requires non-empty unique seeds, positive required budgets, ordered ranges, compatible scheduling/topology choices, explicit blind-versus-instrumented coverage, and non-colliding derived per-seed output identities.

**Rationale:** These are reviewable pre-run decisions; elapsed time, completed seeds, coverage, failures, and reports are observations.

### 5. Finite schedule descriptors are typed but not executed by Nickel

**Choice:** Nickel may author a finite ordered schedule of closed fault descriptor alternatives and validate action-specific fields, VM/link target ranges, partition set shape, time ordering, and profile bounds. Rust converts descriptors into its fault types and remains responsible for applicability, application, observation, and receipts.

**Rationale:** Catching an out-of-range target before a campaign is useful; claiming that the fault fired or had an effect is not a configuration concern.

### 6. Generation is an explicit shell step

**Choice:** A preparation command evaluates the pinned contracts, emits deterministic JSON, and records BLAKE3 identities for source, imports, contract, evaluator/profile, and projection. A pure Rust conversion core maps validated DTOs to runtime configs; a thin shell reads files and invokes Nickel only in explicit generate/check workflows.

**Rationale:** Runtime and replay paths remain independent of a Nickel interpreter.

### 7. Rust revalidates external projections

**Choice:** Rust validators preserve all safety and compatibility checks for supplied JSON. Parity tests prove valid Nickel projections map to equivalent Rust DTOs and rejected Nickel fixture classes also fail at the external JSON boundary where applicable.

**Rationale:** Consumers can bypass the repository's authoring path, so Nickel cannot be the sole runtime defense.

## Risks / Trade-offs

- The CLI has a wide option surface; the initial contract must inventory fields before choosing defaults and cannot silently omit identity-affecting inputs.
- Strong cross-field checks can reject previously tolerated combinations. Migrations need explicit diagnostics rather than implicit coercion.
- Fault descriptor contracts can drift toward execution semantics; applicability and observed outcomes stay Rust-owned and are tested in their existing packages.
- SHA-256 remains only on existing fields that require it. New ChaosControl-owned profile identities use BLAKE3.
