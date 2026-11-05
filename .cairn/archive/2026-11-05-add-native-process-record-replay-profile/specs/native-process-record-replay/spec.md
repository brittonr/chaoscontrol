# Native Process Record Replay Profile Delta

## ADDED Requirements

### Requirement: Versioned descriptor profile identity

r[chaoscontrol.native_process_profile.identity] ChaosControl MUST publish a bounded canonical native-process replay profile that binds the profile schema, version, producer cohort, record schema, replay class, and fixed non-claims, and MUST derive its identity as BLAKE3 over canonical serialization of the validated value.

#### Scenario: Complete profile identifies
- GIVEN a profile with the current schema, version, record schema, cohort, replay class, and non-claims
- WHEN the profile identity is requested
- THEN a BLAKE3 identity is returned over the canonical serialized profile

#### Scenario: Stale profile is denied
- GIVEN a profile with an older schema, version, or record schema
- WHEN validation runs
- THEN it fails before any descriptor is accepted

### Requirement: Descriptor admission is fail-closed

r[chaoscontrol.native_process_profile.admission] Descriptor admission MUST reject a stale descriptor schema, a non-current non-claim set, an empty or over-bound record set, non-BLAKE3 identities, an end tick before the start tick, duplicate or decreasing sequences in one generation, and drifted process ordering before the descriptor is used.

#### Scenario: Sequence regresses
- GIVEN two records for one process generation with equal or decreasing sequence
- WHEN validation runs
- THEN the descriptor is rejected

#### Scenario: Order drifts
- GIVEN records whose (process key, generation) ordering is not monotonic
- WHEN validation runs
- THEN the descriptor is rejected

### Requirement: Records describe bounded lifecycle facts

r[chaoscontrol.native_process_profile.records] Each native-process record MUST bind a stable process key, generation, lifecycle sequence, executable BLAKE3 identity, start and end logical ticks, one exit classification, and at most four BLAKE3 evidence references. Records MUST NOT carry host paths, secret-bearing text, raw environment, or free-form payload.

#### Scenario: Lifecycle record is accepted
- GIVEN a process generation with increasing sequence and a valid exit classification
- WHEN the descriptor is validated
- THEN the record is retained with its exact ticks and references

#### Scenario: Path or secret leaks
- GIVEN a record whose text contains a host path fragment or secret-bearing word
- WHEN validation runs
- THEN the descriptor is rejected

### Requirement: Descriptors stay detached diagnostics

r[chaoscontrol.native_process_profile.detached_nonclaims] A native-process descriptor MUST remain detached diagnostic evidence and MUST NOT satisfy missing semantic observations, become a logical world root, grant authority, permit repair or voting, or establish semantic equivalence.

#### Scenario: Consumer validates a diagnostic member
- GIVEN a descriptor and cohort identity supplied by another repository
- WHEN that repository validates the descriptor
- THEN it may retain the exact identity as an opaque diagnostic only

#### Scenario: Non-claims are altered
- GIVEN a profile whose non-claim set differs from the published set
- WHEN validation runs
- THEN the profile and descriptor are rejected

### Requirement: Positive and negative coverage

r[chaoscontrol.native_process_profile.validation] The focused suite MUST include a positive complete descriptor plus negatives for stale schema or version, record-set bounds, duplicate or decreasing sequence, drifted order, non-BLAKE3 identities, reversed ticks, evidence-ref overflow, host path, secret-bearing text, and a changed non-claim set.

#### Scenario: Focused suite runs
- GIVEN the protocol crate tests are executed
- WHEN the native-process module runs
- THEN every positive and negative case passes at its declared boundary
