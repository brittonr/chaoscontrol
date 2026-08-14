## Why

ChaosControl's exploration loop combines reusable frontier policy with VM snapshots, fault-schedule mutation, input-tree discovery, KVM execution, coverage, assertions, findings, and evidence.

Campaign will own the product-neutral policy only. Choregraph will own immutable exploration history and branch references. ChaosControl must retain every VM and product authority.

The existing `campaign.rs` multi-seed runner is not the primary extraction target. It remains a ChaosControl execution and aggregation shell.

## What Changes

- Record the exact adaptive exploration baseline in `frontier.rs`, `input_tree.rs`, and `explorer.rs`.
- Add pinned Campaign and Choregraph history dependencies only after both publish compatible immutable revisions.
- Map ChaosControl frontier moments and expansion operations to opaque Campaign candidates over Choregraph history projections.
- Convert floating-point product scores to bounded integer ranks under a versioned ChaosControl adapter profile.
- Supply explicit entropy tickets for epsilon-style selection and bind them to deterministic source identities.
- Publish exact selection events and fenced Choregraph control-branch changes before KVM expansion work.
- Keep snapshots, schedules, choice histories, VM execution, coverage, assertions, findings, replay, minimization, and evidence in ChaosControl.
- Compare the legacy frontier and Campaign-backed policy over bounded positive and negative model fixtures plus a selected KVM exploration smoke.
- Retain the legacy policy only as explicit diagnostic rollback code after cutover.

## Impact

- **Primary sources**: `crates/chaoscontrol-explore/src/frontier.rs`, `input_tree.rs`, and `explorer.rs`
- **Excluded source**: multi-seed lifecycle and aggregation in `campaign.rs`
- **External dependencies**: immutable published Campaign and Choregraph history revisions, not sibling paths
- **Compatibility**: frontier choice, score decay, epsilon exploration, pruning, stop classes, and product observations
- **Testing**: focused baseline, Campaign conformance, model parity, negative fixtures, KVM exploration smoke, Octet, Clippy, Cairn, and Nix gates
- **Current blocker**: Choregraph branchable history is not published, and Campaign has no implementation revision
