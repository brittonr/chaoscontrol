## Phase 1: Baselines and blockers

- [x] [serial] r[chaoscontrol.campaign_policy.baseline] Record exact `frontier.rs`, `input_tree.rs`, and `explorer.rs` positive and negative baseline results. See `verification-2026-09-07.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.boundary] Record why `campaign.rs` multi-seed execution and aggregation remain outside Campaign adoption. See `verification-2026-09-07.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.source_pins] Record the current Campaign and Choregraph publication blockers and required evidence. The historical blockers and the scoped client repair are recorded in `transport-review.md` and `public-acquisition-2026-09-07.md`.
- [x] [depends:chaoscontrol.campaign_policy.source_pins] r[chaoscontrol.campaign_policy.source_pins] Add exact Cargo and Nix revisions after both compatible dependencies publish. Fresh locked acquisition, 211 library tests, and focused strict Clippy pass from frozen code `3cc7324c`. See `public-acquisition-2026-09-07.md`. Development re-pins since then move only Cargo (Campaign `c5e20cf2`, commit `a0b1b3c`); Nix still names `e23e3edf`, and the release re-pin is the open Phase 5 task.

## Phase 2: Pure product adapter

- [x] [depends:chaoscontrol.campaign_policy.source_pins] r[chaoscontrol.campaign_policy.history] Map retained moments and Campaign control events to exact Choregraph identities. Commits `fb9a221`, `3314e8b`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.history] r[chaoscontrol.campaign_policy.adapter] Map frontier entries to reusable opaque candidates without product DTO leakage. Commits `fb9a221`, `2da6277`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.ranks] Implement versioned guidance records and checked score-to-rank conversion. Commits `fb9a221`, `3314e8b`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.entropy] Bind seeded epsilon entropy to exact Campaign selection inputs. Commits `fb9a221`, `b6c9194`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.adapter] r[chaoscontrol.campaign_policy.snapshot_eligibility] Require exact restorable snapshot or clean-bootstrap eligibility facts. Commits `fb9a221`, `3314e8b`. See `verification.md`.
- [x] [parallel] r[chaoscontrol.campaign_policy.adapter.invalid] Add crossed moment, stale state, duplicate candidate, malformed rank, overflow, and ineligible snapshot fixtures. Commits `fb9a221`, `2da6277`, `b6c9194`. See `verification.md`.

## Phase 3: Selection and exploration shell

- [x] [serial] r[chaoscontrol.campaign_policy.publication_fence] Resolve durable journal authority for `output_dir=None` before changing accepted run behavior. The user approved rejection before KVM work without explicit journal authority. See `durable-storage-decision.md`.

- [x] [depends:chaoscontrol.campaign_policy.ranks] [depends:chaoscontrol.campaign_policy.entropy] r[chaoscontrol.campaign_policy.frontier_parity] Map score decay, ranked choice, exploratory choice, and pruning to Campaign decisions. Commits `3314e8b`, `3b5cb62`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.frontier_parity] r[chaoscontrol.campaign_policy.publication_fence] Publish the exact selection event and fenced control-branch move before expansion. Commits `a60cccd`, `3314e8b`, `d0719c5`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.publication_fence] r[chaoscontrol.campaign_policy.effects] Execute mutation, input-tree selection, workers, snapshots, and KVM branches only in ChaosControl. Commits `3314e8b`, `d0719c5`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.effects] r[chaoscontrol.campaign_policy.observations] Map bounded child moments, guidance updates, costs, findings, and outstanding-selection closure after product evaluation. Commits `fb9a221`, `3314e8b`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.observations] r[chaoscontrol.campaign_policy.stop] Preserve frontier, budget, plateau, finding, signal, and maximum-round stop mappings. Commits `fb9a221`, `a8d2716`, `3314e8b`, `b6c9194`. See `verification.md`.
- [x] [parallel] r[chaoscontrol.campaign_policy.publication_fence.invalid] Add unpublished, stale-generation, stale-head, duplicate-selection, and execution-before-publication fixtures. Commits `fb9a221`, `2da6277`, `a60cccd`, `3314e8b`. See `verification.md`.

## Phase 4: Product authority and compatibility

- [x] [serial] r[chaoscontrol.campaign_policy.product_authority] Keep schedules, choice histories, snapshots, coverage, assertions, findings, replay, minimization, and evidence local. Commits `3314e8b`, `b6c9194`. See `verification.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.progress] Keep checkpoints, multi-seed progress, resume, aggregation, and report projection local. Commits `31e6a40`, `1ba05d1`, `420cb05`. See `verification.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.evidence] Reject Campaign and Choregraph structural receipts as VM, fault, assertion, finding, replay, or release evidence. Commits `2da6277`, `1c06a11`. See `verification.md`.
- [ ] [parallel] r[chaoscontrol.campaign_policy.evidence.invalid] Add receipt-promotion, stale-cache, missing-snapshot, false-observation, and product-type leakage fixtures. The first four exist; the product-type leakage fixture is missing. See `verification.md`.

## Phase 5: Parity and cutover

- [x] [depends:chaoscontrol.campaign_policy.stop] r[chaoscontrol.campaign_policy.frontier_parity] Compare legacy and shared policy over bounded positive and negative model fixtures. Commits `3b5cb62`, `b6c9194`, `c6c81cc`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.frontier_parity] r[chaoscontrol.campaign_policy.kvm_parity] Run one selected adaptive KVM exploration smoke on a compatible host. Commit `d0719c5`. See `kvm-smoke-2026-09-25.md`.
- [x] [depends:chaoscontrol.campaign_policy.kvm_parity] r[chaoscontrol.campaign_policy.cutover] Select the shared policy only after conformance, model parity, and KVM evidence pass. Commit `420cb05`. See `verification.md`.
- [x] [depends:chaoscontrol.campaign_policy.cutover] r[chaoscontrol.campaign_policy.rollback] Remove legacy policy or retain it only as explicit diagnostic rollback code. Commits `be95656`, `420cb05`. See `verification.md`.
- [ ] [depends:chaoscontrol.campaign_policy.rollback] r[chaoscontrol.campaign_policy.source_pins] Re-pin Cargo and the Nix `campaign-src` input to the same Campaign release revision.
- [ ] [serial] r[chaoscontrol.campaign_policy.source_pins] Run focused tests, Clippy, Octet, Cairn, Campaign conformance, parity, KVM smoke, and `nix flake check -L`.
