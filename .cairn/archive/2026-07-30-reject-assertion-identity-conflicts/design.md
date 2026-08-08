## Context

`CatalogEntry` calls its `u32` FNV-1a value unique, but no collision check exists. `FaultEngine::handle_hypercall` decodes guest/category while discarding catalog file/line, and `PropertyOracle` stores `BTreeMap<u32, AssertionRecord>`. Catalog and runtime paths keep the first record or mutate it without comparing kind/message metadata. `SimulationController::merged_oracle_report` aggregates records with the same integer across VMs, while `sdk_local_report` keeps the first JSONL descriptor under an ID string.

The system needs a catalog-validation phase and an identity type that survives every transport and report boundary.

## Decisions

### 1. Separate logical key, descriptor, and fingerprint

**Choice:** Define a versioned `AssertionLogicalKey` containing a catalog namespace plus either an explicit stable key or an automatic source-site key. Define `AssertionDescriptor` as that key plus assertion kind, message, normalized source metadata, guest, and category. Canonical descriptor bytes have one deterministic encoding.

The compact `AssertionFingerprint` is BLAKE3 over domain-separated canonical descriptor bytes. Registries retain the canonical descriptor and compare it whenever fingerprints or logical keys match.

**Rationale:** A digest is efficient on the wire, but only retained canonical data can distinguish an actual duplicate from a key conflict or theoretical digest collision.

### 2. Remove integer identity at the source boundary

**Choice:** Remove public `u32` assertion functions, explicit-ID macros, SDK legacy catalog registration, compatibility command aliases, and unbound guidance APIs. Source code uses automatic identities or explicit stable namespace/key identities.

Automatic source-site keys are build-scoped because file paths and line numbers can move. Stable namespace/key APIs provide deliberate logical continuity. Full fingerprints still change when descriptor metadata changes.

**Rationale:** A source compatibility layer would preserve the ambiguity that this change must remove.

### 3. Validate the complete catalog before accepting events

**Choice:** SDK initialization emits a versioned catalog boundary, all descriptors, and a catalog-complete record with canonical catalog identity. A pure validator checks field bounds, known kinds/categories, canonical ordering/encoding, duplicate equivalence, logical-key conflicts, and fingerprint collisions. Any `LegacyU32` descriptor rejects strict admission.

The host marks a catalog active only after validation succeeds. Strict runtime events before completion, after conflict, or for an unknown identity are rejected and make the run ineligible for accepted assertion evidence.

**Rationale:** Runtime `or_insert_with` cannot safely decide descriptor identity from a hit message alone.

### 4. Bind events to validated descriptors

**Choice:** Runtime assertion events carry the validated descriptor fingerprint or an ephemeral token derived from the accepted catalog plus fingerprint. The oracle resolves it to the retained descriptor and updates only that record. Event kind or descriptor metadata cannot override catalog authority.

The oracle no longer auto-creates assertion records in strict mode. It has no live legacy record map or integer recording methods. Per-run hits, satisfaction, and immediate failures retain structured fingerprints. Unbound events and old strict-command payloads fail closed. A bounded historical parser can classify diagnostic input, but it cannot update live records or counters.

The old guidance command stored values in `HashMap<u32, f64>` without catalog binding. This change removes that API, command, state, and tests. A future design can restore guidance only with an exact catalog token and fingerprint.

**Rationale:** Catalog identity must be authoritative for both exercised and unexercised assertions.

### 5. Make aggregation descriptor-aware

**Choice:** Per-VM reports retain complete assertion identity and the VM-instance dimension. Aggregation combines counts across VM instances only when logical key, canonical descriptor, and fingerprint all match. Distinct catalog namespaces remain distinct. Any conflict is a report error rather than first-wins behavior.

Each merge source must already carry a true collision-safe claim. The validator independently checks all source facts before it checks that claim. Only an internal prepared-output validator accepts a false claim while `PropertyOracle::report` or report merge derives the final true value.

Compatibility aliases remain selectors for validated structured reports. Selector lookup validates the complete final report, rejects legacy or mixed maps and explicit demotion, searches only structured records, and requires a unique alias match.

The local JSONL report uses the same pure registry/merge core as the VMM report path.

**Rationale:** Multiple instances of the same guest binary should aggregate the same property, while unrelated guests and colliding catalogs must not.

### 6. Define strict legacy behavior

**Choice:** Versioned strict protocol and evidence modes require automatic or stable structured identity. Old `u32` source APIs and compatibility wire aliases do not exist. Bounded readers can identify historical `LegacyU32` input only to reject or quarantine it with a `legacy-ambiguous` classification.

Legacy input cannot complete an accepted catalog. It cannot update strict counters, readiness, replay, merges, restore, or promotion. General snapshot validation can classify bounded historical diagnostics. Runtime restore accepts only pristine Pending state or fully validated Accepted structured state, and it validates active-run fingerprints before mutation. Runtime records remain Rust-owned. Nickel rejects `LegacyU32` in accepted summaries.

**Rationale:** Adapting old records would recreate the unsupported uniqueness claim.

### 7. Treat replay carriers as catalog-bound evidence

**Choice:** Bugs, schema-v2 replay verdicts, checkpoints, campaign collections, and minimization inputs carry the complete assertion evidence identity. Replay authority comes only from joining that carrier to a reconstructed accepted catalog or validated restored report. A numeric alias is redundant: it matches a present compatibility ID, or it is zero when no compatibility ID exists.

Collection validation is atomic. Resume, aggregation, auto-minimization preparation, and export reject the complete untrusted collection on the first missing, legacy, malformed, or report-mismatched identity. Historical ID-only bugs and schema-v1 verdicts remain bounded diagnostic input and never become replay or promotion authority.

**Rationale:** Validating canonical bytes alone does not reject a canonical legacy descriptor and does not prove catalog membership. Dropping one invalid bug would erase a failure and make the resulting report look cleaner.

### 8. Keep identity validation pure and adversarially tested

**Choice:** Canonicalization, fingerprint input construction, catalog insertion, conflict classification, event resolution, and report merge are pure. Tests inject candidates with the same test fingerprint but different canonical descriptors to prove collision handling independently of BLAKE3's practical strength.

**Rationale:** Negative collision behavior should be testable without finding a real BLAKE3 collision or mocking I/O.

## Risks / Trade-offs

- Structured identities increase protocol/report size and require coordinated SDK, host, report, and contract migration.
- Automatic IDs will not promise stability across source relocation; explicit logical keys are required for that use case.
- Strict mode rejects previously tolerated unregistered or conflicting events, which can expose latent workload defects.
- BLAKE3 collision resistance reduces accidental collision risk, but the safety property still depends on canonical descriptor comparison and fail-closed validation.
