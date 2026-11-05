## ADDED Requirements

### Requirement: Raw fallback IDs preserve caller identity

r[chaoscontrol.raw_report_ids.exact]

Local fallback declarations and hits for accepted raw assertions MUST preserve the caller-supplied string ID exactly. IDs MUST NOT be replaced with hexadecimal compact aliases. The ID MUST be recovered from the accepted descriptor's stable key in the `raw` namespace rather than untrusted event details. This namespace rule MUST apply equally to compiled and runtime registration. Local evidence catalog projection, event admission and report verification MUST use the same raw ID rule and reject IDs inconsistent with the accepted descriptor.

#### Scenario: Adapter declares and hits a named raw site

- GIVEN an adapter declares a raw site before initialization with a valid named ID, including JSON-escaped characters or Unicode
- WHEN its declaration and subsequent hit are written to local fallback JSONL
- THEN both records contain the original ID after JSON decoding and retain the same accepted assertion fingerprint

### Requirement: Report identity does not alter admission or VM identity

r[chaoscontrol.raw_report_ids.preservation]

Compiled-site local ID formatting outside the `raw` namespace and fingerprint fallback, descriptor-bound source metadata, raw display and must-hit metadata, accepted fingerprints and VM compatibility aliases MUST remain unchanged. Local evidence MUST admit catalog-bound `hit = false`, `condition = false` declarations for every assertion kind without counting them as observed hits. Declarations with true conditions, inconsistent IDs or mismatched bound metadata MUST remain invalid. Undeclared late raw sites MUST remain rejected. Local-only ID formatting MUST NOT allocate a discarded report string for VM wire emission.

#### Scenario: Raw ID preservation remains scoped to accepted raw sites

- GIVEN ordinary compiled sites and accepted raw-namespace sites coexist
- WHEN local records are emitted and an undeclared late raw site is attempted
- THEN ordinary compiled IDs retain their existing representation, raw records retain their metadata and no accepted record is emitted for the late site
