## Why

ChaosControl's exploration loop combines reusable frontier policy with VM snapshots, fault-schedule mutation, input-tree discovery, KVM execution, coverage, assertions, findings, and evidence.

Campaign will own the product-neutral policy only. Choregraph will own immutable exploration history and branch references. ChaosControl must retain every VM and product authority.

The existing `campaign.rs` multi-seed runner is not the primary extraction target. It remains a ChaosControl execution and aggregation shell.

## What Changes

- Record the exact adaptive exploration baseline in `frontier.rs`, `input_tree.rs`, and `explorer.rs`.
- Pin published Campaign and Choregraph history revisions in Cargo and Nix, never sibling paths.
- Map ChaosControl frontier moments and expansion operations to opaque Campaign candidates over Choregraph history projections.
- Convert floating-point product scores to bounded integer ranks under a versioned ChaosControl adapter profile.
- Supply explicit entropy tickets for epsilon-style selection and bind them to deterministic source identities.
- Publish exact selection events and fenced Choregraph control-branch changes before KVM expansion work.
- Keep snapshots, schedules, choice histories, VM execution, coverage, assertions, findings, replay, minimization, and evidence in ChaosControl.
- Compare the legacy frontier and Campaign-backed policy over bounded positive and negative model fixtures plus a selected KVM exploration smoke.
- Select the Campaign-backed policy by default after conformance, model parity, and KVM evidence pass. Retain the legacy policy only as explicit, release-ineligible diagnostic rollback code.

## Impact

- **Primary sources**: `crates/chaoscontrol-explore/src/frontier.rs`, `input_tree.rs`, and `explorer.rs`
- **Excluded source**: multi-seed lifecycle and aggregation in `campaign.rs`
- **External dependencies**: immutable published Campaign and Choregraph history revisions, not sibling paths
- **Compatibility**: frontier choice, score decay, epsilon exploration, pruning, stop classes, and product observations
- **Testing**: focused baseline, Campaign conformance, model parity, negative fixtures, KVM exploration smoke, Octet, Clippy, Cairn, and Nix gates
- **Current state**: Choregraph history `b3e08e19` and Campaign `e23e3edf` are published and pass fresh locked acquisition from their canonical endpoints (`public-acquisition-2026-09-07.md`). The pure adapter, durable journal, publication fence, parity corpus, KVM smoke, cutover, and product-type leakage fixtures are committed. Cargo and the Nix `campaign-src` input both pin the final Campaign revision `e7757a51` (R5), which carries the round-12 conformance repairs under Campaign's D99 closure; no further Campaign review round runs (earlier development pins: `c5e20cf2`, `260eb9ba`, `99f8eff3`, `a75b5e3a`, `3a182d5a`, `9f1f33a6`, `1f8e1a74`, `65eee6e7`). See `design.md` and `verification.md`.
- **Remaining obligations**: fresh acquisition of `e7757a51` from the live endpoint and the full `nix flake check -L` gate. The forge answered HTTP 530 when `260eb9ba`, `99f8eff3`, `a75b5e3a`, `3a182d5a`, `9f1f33a6`, `1f8e1a74`, `65eee6e7`, and `e7757a51` were pinned. The KVM smoke at the R5 head is recorded in `kvm-smoke-2026-09-26.md`. Until both obligations are met, this change makes no release claim.
