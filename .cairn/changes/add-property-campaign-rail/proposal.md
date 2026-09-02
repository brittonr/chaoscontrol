# Proposal: Deterministic property-campaign falsification rail

## Why

ChaosControl owns coverage-guided exploration, fault schedules, minimizer work, and replay for Rust workloads. It lacks a first-class property-campaign rail: declared invariants, agent-synthesized generators, differential oracles, and minimal-counterexample receipts. MoonBit's `core/quickcheck` work demonstrates deterministic, seeded falsification with failure-mode-preserving shrinking as a toolchain-integrated practice. ChaosControl should own the same discipline as a bounded Rust rail without broadening its product scope.

## What Changes

- Add a typed property-campaign contract for declared properties, generators, oracle kind, seeds, bounds, and verdict classes. r[chaoscontrol.property_campaign.campaign]
- Execute every campaign from recorded seeds so a rerun reproduces the exact sample and counterexample sequences. r[chaoscontrol.property_campaign.seeded]
- Support invariant, round-trip, and differential oracles; a differential oracle compares against a naive reference model bound to the campaign. r[chaoscontrol.property_campaign.oracles]
- Admit agent-synthesized properties and generators only through the campaign contract with recorded provenance and distribution profiles. r[chaoscontrol.property_campaign.synthesis]
- Route minimization through the existing reducer core so shrinking preserves failure modes and stays bound-checked. r[chaoscontrol.property_campaign.minimize]
- Emit bounded receipts binding seed, verifier kind, generator identity, oracle identity, verdict, and the minimal counterexample without raw sample dumps. r[chaoscontrol.property_campaign.evidence]
- Add positive and negative fixtures for passing, failing, shrinking, mode-preservation, staleness, and overclaim cases. r[chaoscontrol.property_campaign.fixtures]

## Impact

- **Core**: campaign admission, seed and identity construction, oracle normalization, verdict classification, and receipt payload construction stay pure deterministic logic.
- **Shell**: harness execution, process invocation, and output rendering stay in the shell.
- **Existing owners**: the reducer core, deterministic simulation core, and evidence classes keep their claims.

## Lifecycle Prerequisites

None. The rail composes existing ChaosControl determinism and reducer capabilities.

## Out of Scope

- New fuzzing engines, guest SDK changes, theorem proving, or hosted-product scope.
- Changes to VM snapshot replay proof, package trust, or release eligibility.

## Affected Specs

- `property-campaign`: campaign, seeded, oracles, synthesis, minimize, evidence, fixtures, and non-claims.
