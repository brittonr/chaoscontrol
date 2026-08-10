## Context

ChaosControl pins Mantle commit `a141fcbaafe41f9a413a81275a33fe915bfca370` and NASA SpaceWasm commit `e24cf09355a90497148eb5029fdb8e3400bd63e3`. The Mantle derivation still verifies its complete bundle, and the host-runner identity remains unchanged. However, the checked-in consumer profile carries stale manifest and bundle identities.

## Decisions

### Decision: Refresh only from the verified immutable bundle

**Choice:** Admit the manifest BLAKE3 and bundle identity read from the exact pinned Mantle output after its own materialization verifier succeeds. Update every consumer projection in one change and rerun the full focused differential rail.

**Rationale:** Changing only an expected digest would hide provenance. Binding the exact source revision, full verified manifest, bundle identity, stable runner identity, and successful consumer execution preserves fail-closed admission.

### Decision: Keep the profile schema and claim boundary stable

**Choice:** Do not change runtime behavior, schema fields, comparison semantics, bounds, or non-claims. Treat this as cohort re-admission, not compatibility expansion.

**Rationale:** The mismatch is an evidence identity drift. It does not justify a broader runtime or correctness claim.

## Risks / Trade-offs

- A producer bundle can drift again if its full dependency closure is not stable. Future refreshes must compare immutable source and runner identities and retain exact remeasurement evidence.
- Passing differential checks remains diagnostic agreement for the bounded corpus. It does not prove either runtime correct.
