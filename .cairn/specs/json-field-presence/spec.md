# Json Field Presence Specification

## Purpose

Defines the `json-field-presence` capability.

## Requirements

### Requirement: Structured event fields support bounded presence selection

r[chaoscontrol.json_field_presence.selection]

JsonPresent and JsonTextPresent MUST test whether a confined JSON pointer resolves in canonical details and parsed canonical event text, respectively, without changing rows. Any resolved value, including null, false, zero and empty strings/containers, MUST count as present. Missing/untraversable paths and absent details MUST be false. Not MUST express absence without changing existing predicate semantics. Visited invalid/trailing/over-deep JSON text MUST fail with InvalidJsonText; skipped text MUST not be parsed, while skipped predicate structure MUST still be admitted. Existing pointer/text, operation, structural, moment and receipt bounds MUST remain effective.

#### Scenario: Existing values are not confused with absence

- GIVEN admitted events with structured values, null and missing paths
- WHEN presence or its negation filters either supported source
- THEN all resolved values are present, missing paths are absent, and canonical rows remain unchanged independently of the other source or derived strings

#### Scenario: Presence guards compose with negative queries

- GIVEN events with and without a structured metric
- WHEN presence is conjoined with a negated numeric predicate
- THEN missing metrics are excluded, and existing values follow the unchanged numeric predicate semantics

#### Scenario: Parsing, bounds and receipt drift fail explicitly

- GIVEN presence queries and source/query/result-bound receipts under declared limits
- WHEN visited text is invalid, a pointer or resource/structural bound fails, or a bound fact drifts
- THEN evaluation or receipt validation rejects the input; CLI evaluation failures emit no partial artifacts and skipped branches do not bypass structural admission
