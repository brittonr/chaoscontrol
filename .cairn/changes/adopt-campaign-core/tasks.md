## Phase 1: Baselines and blockers

- [ ] [serial] r[chaoscontrol.campaign_policy.baseline] Record exact `frontier.rs`, `input_tree.rs`, and `explorer.rs` positive and negative baseline results.
- [ ] [serial] r[chaoscontrol.campaign_policy.boundary] Record why `campaign.rs` multi-seed execution and aggregation remain outside Campaign adoption.
- [ ] [serial] r[chaoscontrol.campaign_policy.source_pins] Record the current Campaign and Choregraph publication blockers and required evidence.
- [ ] [depends:chaoscontrol.campaign_policy.source_pins] r[chaoscontrol.campaign_policy.source_pins] Add exact Cargo and Nix revisions after both compatible dependencies publish.

## Phase 2: Pure product adapter

- [ ] [depends:chaoscontrol.campaign_policy.source_pins] r[chaoscontrol.campaign_policy.history] Map retained moments and Campaign control events to exact Choregraph identities.
- [ ] [depends:chaoscontrol.campaign_policy.history] r[chaoscontrol.campaign_policy.adapter] Map frontier entries to reusable opaque candidates without product DTO leakage.
- [ ] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.ranks] Implement versioned guidance records and checked score-to-rank conversion.
- [ ] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.entropy] Bind seeded epsilon entropy to exact Campaign selection inputs.
- [ ] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.snapshot_eligibility] Require exact restorable snapshot or clean-bootstrap eligibility facts.
- [ ] [parallel] r[chaoscontrol.campaign_policy.adapter.invalid] Add crossed moment, stale state, duplicate candidate, malformed rank, overflow, and ineligible snapshot fixtures.

## Phase 3: Selection and exploration shell

- [ ] [depends:chaoscontrol.campaign_policy.ranks] [depends:chaoscontrol.campaign_policy.entropy] r[chaoscontrol.campaign_policy.frontier_parity] Map score decay, ranked choice, exploratory choice, and pruning to Campaign decisions.
- [ ] [depends:chaoscontrol.campaign_policy.frontier_parity] r[chaoscontrol.campaign_policy.publication_fence] Publish the exact selection event and fenced control-branch move before expansion.
- [ ] [depends:chaoscontrol.campaign_policy.publication_fence] r[chaoscontrol.campaign_policy.effects] Execute mutation, input-tree selection, workers, snapshots, and KVM branches only in ChaosControl.
- [ ] [depends:chaoscontrol.campaign_policy.effects] r[chaoscontrol.campaign_policy.observations] Map bounded child moments, guidance updates, costs, findings, and outstanding-selection closure after product evaluation.
- [ ] [depends:chaoscontrol.campaign_policy.observations] r[chaoscontrol.campaign_policy.stop] Preserve frontier, budget, plateau, finding, signal, and maximum-round stop mappings.
- [ ] [parallel] r[chaoscontrol.campaign_policy.publication_fence.invalid] Add unpublished, stale-generation, stale-head, duplicate-selection, and execution-before-publication fixtures.

## Phase 4: Product authority and compatibility

- [ ] [serial] r[chaoscontrol.campaign_policy.product_authority] Keep schedules, choice histories, snapshots, coverage, assertions, findings, replay, minimization, and evidence local.
- [ ] [serial] r[chaoscontrol.campaign_policy.progress] Keep checkpoints, multi-seed progress, resume, aggregation, and report projection local.
- [ ] [serial] r[chaoscontrol.campaign_policy.evidence] Reject Campaign and Choregraph structural receipts as VM, fault, assertion, finding, replay, or release evidence.
- [ ] [parallel] r[chaoscontrol.campaign_policy.evidence.invalid] Add receipt-promotion, stale-cache, missing-snapshot, false-observation, and product-type leakage fixtures.

## Phase 5: Parity and cutover

- [ ] [depends:chaoscontrol.campaign_policy.stop] r[chaoscontrol.campaign_policy.frontier_parity] Compare legacy and shared policy over bounded positive and negative model fixtures.
- [ ] [depends:chaoscontrol.campaign_policy.frontier_parity] r[chaoscontrol.campaign_policy.kvm_parity] Run one selected adaptive KVM exploration smoke on a compatible host.
- [ ] [depends:chaoscontrol.campaign_policy.kvm_parity] r[chaoscontrol.campaign_policy.cutover] Select the shared policy only after conformance, model parity, and KVM evidence pass.
- [ ] [depends:chaoscontrol.campaign_policy.cutover] r[chaoscontrol.campaign_policy.rollback] Remove legacy policy or retain it only as explicit diagnostic rollback code.
- [ ] [serial] r[chaoscontrol.campaign_policy.source_pins] Run focused tests, Clippy, Octet, Cairn, Campaign conformance, parity, KVM smoke, and `nix flake check -L`.
