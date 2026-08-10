# SpaceWasm MVP Differential Specification Delta

## ADDED Requirements

### Requirement: Bundle admission refreshes retain remeasurement evidence

r[chaoscontrol.spacewasm_mvp.admission_refresh] A change to an admitted Mantle manifest or bundle identity MUST bind the exact producer revision, producer-verified output, full manifest BLAKE3, bundle identity, runtime identity, unchanged claim boundary, and successful positive and negative consumer validation.

#### Scenario: Verified bundle identity changes
- GIVEN the exact pinned producer emits a complete bundle with a new measured manifest or bundle identity
- WHEN ChaosControl refreshes the admitted cohort
- THEN every typed, generated, test, and documentation projection MUST change together and the focused differential rail MUST pass before admission.

#### Scenario: A digest changes without complete provenance
- GIVEN an expected identity is edited without exact producer verification or matching consumer evidence
- WHEN admission validation runs
- THEN the refresh MUST remain blocked and MUST NOT weaken bundle verification.
