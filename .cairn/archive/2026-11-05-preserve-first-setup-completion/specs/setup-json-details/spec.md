## ADDED Requirements

### Requirement: The first validated setup completion wins

r[chaoscontrol.setup_json_details.first_completion]

Both strict local evidence and SDK diagnostic readers MUST treat repeated valid setup completions as idempotent. The first validated record MUST determine setup adoption metadata and increment the semantic completion count once. Later records MUST NOT replace or inflate setup metadata, count another transition or prevent subsequent assertion/event processing. Explicit JSON details of all supported types MUST retain this behavior. This is a local input-order rule, not a claim about cross-process global ordering or VM execution.

#### Scenario: Initialization components report completion repeatedly

- GIVEN a valid setup record followed by identical or differing valid completion records and a passing assertion/event
- WHEN the stream is reduced by either local reader
- THEN setup is complete once, only the first setup contributes adoption metadata and subsequent assertion/event records remain observed

## MODIFIED Requirements

### Requirement: JSON details do not weaken lifecycle evidence bounds

r[chaoscontrol.setup_json_details.bounds]

The `details` field MUST remain required even though its value may be null. Setup status and unknown-field checks, strict evidence's existing object metadata key/field limits, and each reader's generic JSON/stream resource limits MUST remain enforced on every record, including repeats after completion. Object details MUST retain their existing top-level adoption metadata behavior for the first completion. Evidence remains local instrumentation, not VM or replay proof.

#### Scenario: Completed setup does not bypass validation

- GIVEN a valid setup record followed by a record with omitted details, invalid status, unknown fields, over-bound object metadata or over-bound JSON structures
- WHEN local evidence is admitted
- THEN the invalid stream is rejected; repeated valid records instead follow the first-completion rule
