# Benchmark Bug Corpus Specification

## Purpose

Provide a versioned, repeatable set of known-bug workload entries with expected verdicts and rarity profiles, so exploration changes can be measured and rarity statistics can be validated.

## ADDED Requirements

### Requirement: The corpus manifest is Nickel-owned

r[chaoscontrol.benchmark.manifest] The corpus manifest MUST be authored in Nickel with a typed contract, MUST export a BLAKE3-bound JSON projection for the runner, and MUST reject an entry that lacks an expected verdict.

#### Scenario: Entry without a verdict
- GIVEN a manifest entry with no expected verdict
- WHEN the manifest validates
- THEN the contract MUST reject the entry.

#### Scenario: Valid manifest
- GIVEN a manifest that satisfies the contract
- WHEN the projection exports
- THEN every entry MUST have an id, a class, an expected verdict, and a rarity profile.

### Requirement: The interleaving entry is schedule-sensitive

r[chaoscontrol.benchmark.interleaving] The interleaving entry MUST reproduce its bug only under a specific schedule, MUST ship a positive variant that reproduces, and MUST ship a negative variant that passes.

#### Scenario: Positive reproduction
- GIVEN the interleaving entry under its triggering schedule
- WHEN the runner asserts the verdict
- THEN the expected failure MUST occur.

#### Scenario: Negative control
- GIVEN the interleaving entry without the triggering schedule
- WHEN the runner asserts the verdict
- THEN the entry MUST pass.

### Requirement: The liveness entry is sequence-sensitive

r[chaoscontrol.benchmark.liveness] The liveness entry MUST require a specific sequence to stall, and MUST ship positive and negative variants.

#### Scenario: Positive stall
- GIVEN the liveness entry under its triggering sequence
- WHEN the runner asserts the verdict
- THEN the expected stall MUST occur.

#### Scenario: Negative control
- GIVEN the liveness entry without the triggering sequence
- WHEN the runner asserts the verdict
- THEN the entry MUST pass.

### Requirement: The rarity entry has a measured base probability

r[chaoscontrol.benchmark.rarity] The rarity entry MUST expose a seeded distribution with a measured base probability, and the measurement MUST be re-derived when the harness changes.

#### Scenario: Probability measured
- GIVEN a fixed seed distribution
- WHEN the corpus validates the rarity entry
- THEN the measured probability MUST be reported with the run count used to measure it.

### Requirement: The runner is bounded and binds receipts

r[chaoscontrol.benchmark.runner] The runner MUST execute each entry under declared bounds, MUST assert the expected verdict, and MUST emit a receipt binding the config digest, round identities, and verdict.

#### Scenario: Receipt after a run
- GIVEN a completed entry run
- WHEN the receipt is inspected
- THEN it MUST contain the config digest, round identity, and verdict.

#### Scenario: Unexpected verdict
- GIVEN a run whose verdict differs from the expected verdict
- WHEN the runner finishes
- THEN the runner MUST report the mismatch as a typed corpus failure.

### Requirement: Corpus validation is adversarial

r[chaoscontrol.benchmark.validation] Validation MUST reproduce every entry's expected verdict, MUST include negative variants that are expected to pass, and MUST verify that verdict mismatches are typed.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat corpus results as benchmark evidence
- WHEN runner, receipt, and lifecycle validation runs
- THEN every positive and negative entry MUST produce its expected result.
