# Property Campaign Specification

## ADDED Requirements

### Requirement: Campaigns are typed and bound

r[chaoscontrol.property_campaign.campaign] ChaosControl MUST define a typed property-campaign contract that binds declared properties, generators, oracle kind, recorded seeds, named bounds, and verdict classes, and MUST admit no campaign that omits them.

#### Scenario: Complete campaign facts are admitted

- GIVEN a caller supplies complete bounded in-memory campaign facts
- WHEN campaign admission runs
- THEN the core MUST return deterministic admission facts or typed blockers
- AND it MUST perform no file, process, network, clock, or harness effect.

#### Scenario: Campaign omits a declared property

- GIVEN a campaign candidate lists a generator without any declared property or oracle kind
- WHEN campaign admission runs
- THEN the core MUST reject the campaign.

### Requirement: Execution is reproducible from seeds

r[chaoscontrol.property_campaign.seeded] Every campaign run MUST bind one recorded seed and reproduce the exact sample and counterexample sequences on rerun; a run without a recorded seed MUST be rejected as a campaign fact.

#### Scenario: Seeded campaign reruns identically

- GIVEN a campaign with a recorded seed completes a run
- WHEN the same campaign reruns under the same seed and inputs
- THEN the sample and counterexample sequences MUST match exactly.

#### Scenario: Seed or inputs changed

- GIVEN a rerun claims identity with a different seed or input set
- WHEN replay identity validation runs
- THEN the rerun MUST be rejected as non-identical.

### Requirement: Oracles are typed

r[chaoscontrol.property_campaign.oracles] Campaign oracles MUST be invariant, round-trip, or differential forms, and a differential oracle MUST compare against a naive reference model bound to the campaign; an oracle with an unbound or wrong-kind reference MUST be rejected.

#### Scenario: Correct oracle kind is supplied

- GIVEN a campaign declares a differential oracle with a bound naive reference model
- WHEN oracle admission runs
- THEN the campaign MAY use the oracle.

#### Scenario: Reference model is missing

- GIVEN a differential oracle is declared without a bound reference model
- WHEN oracle admission runs
- THEN the core MUST reject the oracle.

### Requirement: Synthesis is admitted by contract

r[chaoscontrol.property_campaign.synthesis] Agent-synthesized properties and generators MUST enter only through the campaign contract with recorded provenance and distribution profiles, and MUST NOT bypass campaign bounds, seeds, or verdict classification.

#### Scenario: Synthesized inputs carry provenance

- GIVEN a synthesized property or generator carries complete provenance and distribution profiles within bounds
- WHEN synthesis admission runs
- THEN the campaign MAY accept the input.

#### Scenario: Synthesized input bypasses bounds

- GIVEN a synthesized generator ignores campaign bounds or the recorded seed
- WHEN synthesis admission runs
- THEN the campaign MUST reject the input.

### Requirement: Minimization preserves failure modes

r[chaoscontrol.property_campaign.minimize] Counterexample reduction MUST run through the reducer core under step bounds, MUST preserve the original failure mode (logical assertion versus runtime error), and MUST bind every accepted shrink step and the final minimal candidate.

#### Scenario: Shrink candidate preserves the failure mode

- GIVEN a shrink step reproduces the original failure mode under the recorded predicate
- WHEN reduction admission runs
- THEN the reducer MAY accept the step.

#### Scenario: Shrink candidate changes the failure mode

- GIVEN a shrink candidate triggers a different failure mode than the original counterexample
- WHEN reduction admission runs
- THEN the reducer MUST reject the candidate
- AND the failure mode MUST remain the original class.

### Requirement: Receipts are bounded and safe

r[chaoscontrol.property_campaign.evidence] Campaign receipts MUST bind seed, verifier kind, generator identity, oracle identity, verdict class, bounds, and the minimal counterexample while excluding raw sample dumps and transcripts beyond declared bounds.

#### Scenario: Receipt candidate carries a sample dump beyond bound

- GIVEN a receipt candidate packs raw sample streams beyond bounds
- WHEN receipt validation runs
- THEN validation MUST reject or redact the payload before persistence.

### Requirement: Non-claims are preserved

r[chaoscontrol.property_campaign.nonclaims] Passing campaign evidence MUST NOT claim formal correctness, exhaustive input coverage, reducer equivalence, VM replay proof, package trust, semantic equivalence, production readiness, or release eligibility.

#### Scenario: Passing campaign is promoted to formal correctness

- GIVEN a passing campaign receipt is labeled as formal correctness or exhaustive coverage
- WHEN non-claim validation runs
- THEN the evidence MUST fail.

### Requirement: The rail has positive and negative fixtures

r[chaoscontrol.property_campaign.fixtures] The property-campaign rail MUST include positive passing-campaign, reproducible-rerun, accepted-synthesis, and minimal-counterexample fixtures plus negative failing-campaign, mode-preservation, stale-seed, overclaim, and malformed-receipt fixtures.

#### Scenario: Rail is proposed for product use

- GIVEN campaigns, oracles, seeds, receipts, docs, and fixtures are complete
- WHEN focused Cargo, octet, Cairn, and Nix validation runs
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
