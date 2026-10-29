## MODIFIED Requirements

### Requirement: Report identity does not alter admission or VM identity

r[chaoscontrol.raw_report_ids.preservation]

Compiled-site local ID formatting outside the `raw` namespace and fingerprint fallback, descriptor-bound source metadata, raw display and must-hit metadata, and compiled compatibility aliases MUST remain unchanged. Runtime raw descriptors MUST use alias-free stable string identity; their fingerprints and catalog tokens change at that explicit cutover and earlier artifacts MUST NOT be reinterpreted as current identity. Local evidence MUST admit catalog-bound `hit = false`, `condition = false` declarations for every assertion kind without counting them as observed hits. Declarations with true conditions, inconsistent IDs or mismatched bound metadata MUST remain invalid. Undeclared late raw sites MUST remain rejected. Local-only ID formatting MUST NOT allocate a discarded report string for VM wire emission.

#### Scenario: Raw ID preservation remains scoped to accepted raw sites

- GIVEN ordinary compiled sites and accepted raw-namespace sites coexist
- WHEN local records are emitted and an undeclared late raw site is attempted
- THEN ordinary compiled IDs retain their existing representation, raw records retain their metadata and no accepted record is emitted for the late site
