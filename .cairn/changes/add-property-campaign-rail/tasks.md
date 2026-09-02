# Tasks

## Phase 1: Baseline and prerequisites

- [ ] [serial] Record reducer-core, determinism-core, and evidence-class baselines before core changes. r[chaoscontrol.property_campaign.campaign]
- [ ] [serial] Record accepted non-claim boundaries for exploration and replay evidence and keep them unchanged. r[chaoscontrol.property_campaign.nonclaims]

## Phase 2: Core and shell

- [ ] [serial] Define campaign, property, generator, oracle, seed, verdict, counterexample, and receipt DTOs in a focused pure family. r[chaoscontrol.property_campaign.campaign] r[chaoscontrol.property_campaign.evidence]
- [ ] [serial] Implement pure campaign admission, seed and identity construction, oracle normalization, verdict classification, and receipt payload construction. r[chaoscontrol.property_campaign.seeded] r[chaoscontrol.property_campaign.oracles]
- [ ] [serial] Implement agent-synthesized property and generator admission with provenance and distribution-profile recording. r[chaoscontrol.property_campaign.synthesis]
- [ ] [serial] Wire minimization to the reducer core with failure-mode preservation and step bounds. r[chaoscontrol.property_campaign.minimize]
- [ ] [serial] Add the harness shell that executes campaigns from seeds and renders verdicts and minimal counterexamples. r[chaoscontrol.property_campaign.seeded]

## Phase 3: Evidence and isolation

- [ ] [parallel] Add positive passing-campaign, reproducible-rerun, accepted-synthesis, and minimal-counterexample fixtures. r[chaoscontrol.property_campaign.fixtures]
- [ ] [parallel] Add negative failing-campaign, mode-preservation, stale-seed, overclaim, and malformed-receipt fixtures. r[chaoscontrol.property_campaign.fixtures] r[chaoscontrol.property_campaign.evidence] r[chaoscontrol.property_campaign.nonclaims]
- [ ] [serial] Run focused tests before and after changes, Clippy with warnings denied, octet, Cairn validation and gates, and relevant Nix checks. r[chaoscontrol.property_campaign.fixtures]
