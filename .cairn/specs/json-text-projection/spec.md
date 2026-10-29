# Json Text Projection Specification

## Purpose

Defines the `json-text-projection` capability.

## Requirements

### Requirement: Retained JSON log text supports bounded scalar projection

r[chaoscontrol.json_text_projection.mapping]

MapText MUST parse admitted canonical event text as bounded JSON and project a confined pointer's scalar into a named derived string using existing Map scalar semantics. Canonical text/details/source fields and payload identity MUST remain unchanged. Missing/non-scalar pointers in valid JSON MUST preserve existing derived values without insertion or erasure. Invalid/trailing/parser-depth-excess JSON MUST fail evaluation with InvalidJsonText rather than silently skip a row or return partial results. Existing text/field/pointer/derived/result limits, per-row logical operation charges, query node/depth admission, moment propagation and source/query/result-bound receipts MUST remain effective. Map MUST retain its existing details source and behavior.

#### Scenario: JSON stdout selection

- GIVEN retained JSON log text and independently populated canonical details
- WHEN MapText projects a process/component field and a text predicate selects it
- THEN matching events retain every original canonical field and only the declared derived projection is added

#### Scenario: Missing values and invalid input

- GIVEN scalar, missing/non-scalar pointers and malformed/trailing/over-deep JSON text
- WHEN a projection evaluates
- THEN scalars use existing string conversion, valid missing/non-scalar values preserve the derived map and invalid JSON fails without a partial answer

#### Scenario: Bounds, moments and receipt identity

- GIVEN a projection at declared bounds and a valid receipt
- WHEN resource/structural bounds are exceeded, its source text/query/result changes, or incompatible UpTo moments are joined
- THEN admission/evaluation or receipt validation rejects the input
