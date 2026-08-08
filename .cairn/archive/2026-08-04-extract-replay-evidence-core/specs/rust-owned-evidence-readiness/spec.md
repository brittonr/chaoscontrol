## ADDED Requirements

### Requirement: Shared replay/evidence core migration [r[rust-owned-evidence-readiness.shared-core]]
ChaosControl MUST migrate replay/evidence DTO validation that is shared between explorer output and evidence-readiness gates into a Rust-owned pure core crate while preserving operator-facing compatibility.

#### Scenario: Compatibility adapters preserve public artifacts [r[replay-verdicts.shared-core.compatibility]]
- **GIVEN** existing dogfood receipts, replay verdicts, accepted workload proof manifests, and evidence readiness fixtures use current JSON field names
- **WHEN** the shared core migration runs
- **THEN** compatibility adapters preserve those public fields and replay-class semantics unless a separate change admits a breaking schema update

#### Scenario: Readiness validation delegates structured decisions [r[rust-owned-evidence-readiness.shared-core.validation]]
- **GIVEN** an evidence-readiness command validates accepted replay proof coverage
- **WHEN** it checks replay verdicts, artifact hashes, snapshot refs, replay parent snapshot refs, replay classes, and bounded anti-claim text
- **THEN** those structured decisions are delegated to the shared Rust core rather than duplicated in shell glue or a second crate-local DTO model

#### Scenario: Nickel remains review-boundary validation [r[rust-owned-evidence-readiness.shared-core.nickel-boundary]]
- **GIVEN** human-authored run configs, receipts, or review contracts are Nickel-backed
- **WHEN** runtime bug, checkpoint, assertion, or replay records are produced and validated
- **THEN** Rust remains the owner of runtime DTO serialization and validation while Nickel remains a review-boundary contract layer
