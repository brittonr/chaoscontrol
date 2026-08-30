# WalTier DST Reference Specification

## ADDED Requirements

### Requirement: Comparison source is bounded
r[chaoscontrol.waltier_dst.source] ChaosControl MUST record WalTier DST beside Antithesis. The record MUST state its mechanism layer and claim boundary.

#### Scenario: Record is non-parity
- GIVEN a WalTier DST comparison record
- WHEN documentation validation runs
- THEN the record MUST be bounded and non-parity
- AND it MUST NOT add WalTier-specific gates.

### Requirement: Oracle invariants are named
r[chaoscontrol.waltier_dst.oracle] The record MUST name history monotonicity, exact-prefix instance state, and snapshot-object conservation.

#### Scenario: Invariants are documented
- GIVEN the comparison record
- WHEN the oracle section is inspected
- THEN it MUST name the three invariant classes
- AND it MUST NOT claim that the record proves them.

### Requirement: Layer and claim boundaries
r[chaoscontrol.waltier_dst.boundary] Store-seam simulation and KVM guest simulation MUST remain distinct mechanism layers. The record MUST NOT add ChaosControl gates.

#### Scenario: Layer distinction is explicit
- GIVEN the comparison record
- WHEN a reader checks the mechanics section
- THEN it MUST distinguish the store-seam and KVM guest layers
- AND ChaosControl evidence authority MUST stay unchanged.

### Requirement: Verification remains explicit
r[chaoscontrol.waltier_dst.verification] The change MUST preserve existing verification boundaries. Positive and negative documentation checks MUST cover the reference posture.

#### Scenario: Existing posture is preserved
- GIVEN the completed reference record
- WHEN regression and policy checks run
- THEN the Antithesis comparison posture MUST stay intact
- AND the ChaosControl check rail MUST pass with declared non-claims.
