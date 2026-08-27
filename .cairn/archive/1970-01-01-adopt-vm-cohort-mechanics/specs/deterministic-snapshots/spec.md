# Deterministic Snapshot Specification Delta

## ADDED Requirements

### Requirement: Immutable VM Cohort source

r[chaoscontrol.vm_cohort.source] ChaosControl MUST pin Cargo and Nix to VM Cohort revision `ab123e3673b6dd616b3df5d044026b5e85755149`. It MUST reject moving refs and sibling-path release fallbacks.

#### Scenario: Exact source passes

- GIVEN Cargo, lock, Nix, and evidence select the exact revision
- WHEN dependency validation runs
- THEN source agreement MUST pass

#### Scenario: Moving source fails

- GIVEN one declaration selects a branch, tag, sibling path, or different revision
- WHEN dependency validation runs
- THEN supported cohort publication MUST fail

### Requirement: Product-owned snapshot adapter

r[chaoscontrol.vm_cohort.adapter] ChaosControl MUST map exact snapshot and initialized block facts into VM Cohort checkpoint and cohort contracts without exporting fault, scheduler, assertion, coverage, exploration, replay, or evidence types.

#### Scenario: Complete mapping passes

- GIVEN one complete exact snapshot and initialized disk
- WHEN adapter planning runs
- THEN one bounded VM Cohort plan MUST bind the exact immutable bases and compatibility facts

#### Scenario: Consumer policy leaks

- GIVEN the shared dependency requires one ChaosControl policy type
- WHEN boundary validation runs
- THEN publication MUST fail

### Requirement: Exact state restore remains consumer-owned

r[chaoscontrol.vm_cohort.restore] ChaosControl MUST apply its exact vCPU and in-kernel device snapshot state through VM Cohort-owned clone descriptors before activation.

#### Scenario: Exact restore passes

- GIVEN VM Cohort prepared one compatible clone and supplied exact live descriptors
- WHEN ChaosControl applies the admitted snapshot
- THEN restore MUST complete before endpoint binding and activation

#### Scenario: Restore fails

- GIVEN snapshot topology or state does not match the clone
- WHEN restore runs
- THEN the cohort MUST fail and retain exact cleanup obligations

### Requirement: Bounded behavioral parity

r[chaoscontrol.vm_cohort.parity] Legacy and VM Cohort-backed paths MUST agree on bounded read, write, snapshot, restore, divergence, dirty-page, and error observations. Agreement MUST NOT be treated as a correctness proof.

#### Scenario: Migration corpus agrees

- GIVEN both paths run the same bounded inputs
- WHEN normalized observations are compared
- THEN every required row MUST agree

#### Scenario: One row diverges

- GIVEN one accepted result or error class differs
- WHEN parity is evaluated
- THEN shared-path selection MUST fail

### Requirement: Shared mechanism selection preserves authority

r[chaoscontrol.vm_cohort.selection] After parity, ChaosControl MUST select VM Cohort for shared checkpoint and clone mechanics while retaining fault, scheduler, assertion, coverage, exploration, replay, and evidence authority. Legacy duplicate code MAY remain only as named diagnostic rollback behavior.

#### Scenario: Shared path selected

- GIVEN parity, conformance, cleanup, and KVM smoke pass
- WHEN selection is recorded
- THEN VM Cohort MUST own shared mechanics and ChaosControl MUST retain product policy

#### Scenario: Receipt grants product authority

- GIVEN a valid VM Cohort receipt
- WHEN ChaosControl evaluates a fault, replay, or release action
- THEN the receipt MUST NOT authorize that action

### Requirement: Product authority remains in ChaosControl

r[chaoscontrol.vm_cohort.authority] ChaosControl MUST retain fault, scheduler, assertion, coverage, exploration, replay, evidence, and release authority. VM Cohort source, plans, observations, conformance results, and receipts MUST NOT grant that authority.

#### Scenario: Ownership boundary passes

- GIVEN VM Cohort owns only shared cohort mechanics
- WHEN dependency and source boundary validation runs
- THEN no ChaosControl product-policy type MUST appear in VM Cohort

#### Scenario: Receipt overclaims authority

- GIVEN one valid VM Cohort mechanism receipt
- WHEN a consumer treats it as fault, replay, evidence, or release authority
- THEN consumer selection MUST fail

### Requirement: Consumer verification is positive and negative

r[chaoscontrol.vm_cohort.verification] ChaosControl MUST test exact mapping, restore, parity, selection, drift, partial failure, unknown cleanup, leakage, and all claim boundaries.

#### Scenario: Consumer rail runs

- GIVEN positive and negative fixtures bind one exact source cohort
- WHEN consumer verification runs
- THEN it MUST report bounded outcomes and preserve all non-claims
