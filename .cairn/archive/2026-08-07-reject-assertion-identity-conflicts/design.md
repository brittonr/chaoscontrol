## Context

`CatalogEntry` calls its `u32` FNV-1a value unique, but no collision check exists. `FaultEngine::handle_hypercall` decodes guest/category while discarding catalog file/line, and `PropertyOracle` stores `BTreeMap<u32, AssertionRecord>`. Catalog and runtime paths keep the first record or mutate it without comparing kind/message metadata. `SimulationController::merged_oracle_report` aggregates records with the same integer across VMs, while `sdk_local_report` keeps the first JSONL descriptor under an ID string.

The system needs a catalog-validation phase and an identity type that survives every transport and report boundary.

## Decisions

### 1. Separate logical key, descriptor, and fingerprint

**Choice:** Define a versioned `AssertionLogicalKey` containing a catalog namespace plus either an explicit stable key or an automatic source-site key. Define `AssertionDescriptor` as that key plus assertion kind, message, normalized source metadata, guest, and category. Canonical descriptor bytes have one deterministic encoding.

The compact `AssertionFingerprint` is BLAKE3 over domain-separated canonical descriptor bytes. Registries retain the canonical descriptor and compare it whenever fingerprints or logical keys match.

**Rationale:** A digest is efficient on the wire, but only retained canonical data can distinguish an actual duplicate from a key conflict or theoretical digest collision.

### 2. Scope compatibility IDs instead of trusting them

**Choice:** Existing explicit `u32` IDs map to a legacy-key variant within an explicit catalog namespace. Repeating one with an identical descriptor is idempotent; repeating it with different kind, message, source, guest, or category is a catalog conflict. New explicit-key APIs accept a stable namespace/key suitable for cross-build continuity.

Automatic source-site keys are documented as build-scoped because file paths and line numbers can move. Normalization removes configured build-root prefixes and ambiguous path forms, but does not claim semantic continuity after source edits.

**Rationale:** This preserves source compatibility where possible without pretending that an unnamespaced small integer is globally unique.

### 3. Validate the complete catalog before accepting events

**Choice:** SDK initialization emits a versioned catalog boundary, all descriptors, and a catalog-complete record with canonical catalog identity. A pure validator checks field bounds, known kinds/categories, canonical ordering/encoding, duplicate equivalence, logical-key conflicts, fingerprint collisions, and legacy-alias conflicts.

The host marks a catalog active only after validation succeeds. Strict runtime events before completion, after conflict, or for an unknown identity are rejected and make the run ineligible for accepted assertion evidence.

**Rationale:** Runtime `or_insert_with` cannot safely decide descriptor identity from a hit message alone.

### 4. Bind events to validated descriptors

**Choice:** Runtime assertion events carry the validated descriptor fingerprint or an ephemeral token derived from the accepted catalog plus fingerprint. The oracle resolves it to the retained descriptor and updates only that record. Event kind or descriptor metadata cannot override catalog authority.

The oracle no longer auto-creates assertion records in strict mode. A separate diagnostic mode may quarantine unregistered legacy events, but those events remain visibly unverified.

**Rationale:** Catalog identity must be authoritative for both exercised and unexercised assertions.

### 5. Make aggregation descriptor-aware

**Choice:** Per-VM reports retain complete assertion identity and the VM-instance dimension. Aggregation combines counts across VM instances only when logical key, canonical descriptor, and fingerprint all match. Distinct catalog namespaces remain distinct. Any conflict is a report error rather than first-wins behavior.

The local JSONL report uses the same pure registry/merge core as the VMM report path.

**Rationale:** Multiple instances of the same guest binary should aggregate the same property, while unrelated guests and colliding catalogs must not.

### 6. Define strict legacy behavior

**Choice:** Versioned strict protocol and evidence modes require structured identity. Legacy `u32`-only streams may be parsed in an explicit diagnostic mode scoped by source stream, with metadata consistency checks and a `legacy-ambiguous` classification. They cannot pass collision-safe assertion-evidence acceptance without migration to a validated catalog.

Runtime records remain Rust-owned. Nickel contracts at review boundaries validate the identity version, fingerprint shape, descriptor fields, catalog status, and legacy classification.

**Rationale:** Silently upgrading old records would recreate the unsupported uniqueness claim.

### 7. Keep identity validation pure and adversarially tested

**Choice:** Canonicalization, fingerprint input construction, catalog insertion, conflict classification, event resolution, and report merge are pure. Tests inject candidates with the same test fingerprint but different canonical descriptors to prove collision handling independently of BLAKE3's practical strength.

**Rationale:** Negative collision behavior should be testable without finding a real BLAKE3 collision or mocking I/O.

## Risks / Trade-offs

- Structured identities increase protocol/report size and require coordinated SDK, host, report, and contract migration.
- Automatic IDs will not promise stability across source relocation; explicit logical keys are required for that use case.
- Strict mode rejects previously tolerated unregistered or conflicting events, which can expose latent workload defects.
- BLAKE3 collision resistance reduces accidental collision risk, but the safety property still depends on canonical descriptor comparison and fail-closed validation.
