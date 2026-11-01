# Semantic History Specification Delta

## ADDED Requirements

### Requirement: Provider contract validates retained proofs

r[chaoscontrol.semantic_history.provider_contract] ChaosControl MUST expose a pure provider contract that imports only exact history schema versions 1 and 2 without converting either identity, exports the original schema, and checks admitted histories into reports. Report validation MUST recompute the deterministic bounded checker result and MUST reject any difference in verdict, witness, invalid or unknown proof, reduction, or bound source identity. Reference adapter import MUST reject non-increasing indices and completion metadata that differs from its earlier invocation. Synthetic fixtures MUST NOT be presented as guest observations, execution receipts, or reference-tool evidence.

#### Scenario: Unsupported schema version is imported
- GIVEN a history whose `schema_version` is missing or not 1 or 2
- WHEN provider import runs
- THEN it rejects the history without migration.

#### Scenario: Report retains a forged proof
- GIVEN a report whose verdict, witness state, invalid proof, or unknown proof differs from the deterministic checker result for its bound history
- WHEN report validation runs
- THEN validation fails even though the report shape is valid.

#### Scenario: Source identity changes after checking
- GIVEN a report produced for one history and a history with a changed source artifact
- WHEN report validation runs
- THEN validation fails.

#### Scenario: Reference completion metadata is mismatched
- GIVEN a reference completion whose process, key, or function differs from its earlier invocation
- WHEN reference import runs
- THEN import fails instead of discarding the mismatched metadata.
