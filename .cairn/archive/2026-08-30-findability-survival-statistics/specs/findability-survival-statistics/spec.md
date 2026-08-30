# Findability Survival Statistics Specification

## Purpose

Produce bounded statistical claims about whether a rare bug remains in the system, derived from first-bug-per-subtree observations across exploration rounds.

## ADDED Requirements

### Requirement: Observations are typed and deduplicated

r[chaoscontrol.findability.observations] The core MUST accept typed observations where each subtree contributes its first bug instance only, and MUST reject observations that cannot be attributed to a subtree.

#### Scenario: Duplicate observation in one subtree
- GIVEN two bug records in the same subtree
- WHEN the model assembles observations
- THEN the core MUST count only the first instance.

#### Scenario: Unattributable observation
- GIVEN a bug record with no subtree identity
- WHEN the model assembles observations
- THEN the core MUST reject the observation with a typed error.

### Requirement: The exponential model is pure

r[chaoscontrol.findability.model] The core MUST fit the bug rate as M over T, MUST return the mean time-to-bug as T over M, and MUST return an explicit unbounded result when M is zero.

#### Scenario: Rate from known observations
- GIVEN M first-bug instances and total survival time T
- WHEN the model fits
- THEN the rate MUST equal M over T and the mean time-to-bug MUST equal T over M.

#### Scenario: No bug observed
- GIVEN zero first-bug instances across all subtrees
- WHEN the model fits
- THEN the core MUST report an unbounded estimate and MUST not invent a rate.

### Requirement: Confidence uses a conservative tail

r[chaoscontrol.findability.confidence] The core MUST place a gamma prior on the rate, MUST report p_survival through the resulting Lomax posterior, and MUST state the confidence level, the projected runs to reach it, and the constant-discovery-rate assumption.

#### Scenario: Projection to a stated confidence
- GIVEN a fitted model and a confidence threshold
- WHEN the projection runs
- THEN the report MUST state the number of projected runs at that confidence and the stated assumption.

### Requirement: Independence violations are flagged

r[chaoscontrol.findability.independence] The core MUST flag conditions where a bug is baked into every subtree, and MUST label those subtrees instead of reporting a confidence the model does not support.

#### Scenario: Baked-in bug
- GIVEN a bug present in every subtree of a generation
- WHEN the model runs
- THEN the report MUST flag the independence violation.

### Requirement: Core and shell stay separated

r[chaoscontrol.findability.boundary] The core MUST accept typed observations and MUST not read files, clocks, or processes. The shell MUST assemble observations from round artifacts and MUST validate the observation identities.

#### Scenario: Core reads no artifacts
- GIVEN a direct call to the core
- WHEN the core runs
- THEN it MUST not perform file, clock, or process access.

### Requirement: Findability evidence binds to inputs

r[chaoscontrol.findability.evidence_binding] Findability receipts MUST bind the observation identity, model parameters, and outputs with BLAKE3 identities, and MUST fail closed on identity drift.

#### Scenario: Observation identity drifts
- GIVEN a receipt whose observation identity differs from the assembled rounds
- WHEN receipt validation runs
- THEN validation MUST fail closed.

### Requirement: Findability validation is adversarial

r[chaoscontrol.findability.validation] Validation MUST pair a positive known-probability fixture with negative fixtures for empty data, a single observation, no-bug generations, and a baked-in bug.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat findability reports as evidence
- WHEN core, shell, receipt, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
