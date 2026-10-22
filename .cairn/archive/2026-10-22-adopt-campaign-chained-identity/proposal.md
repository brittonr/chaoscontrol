## Why

ChaosControl pins Campaign development revision `e7757a5134f631e1ed410df3cae57e57f887c468` from `https://git.onix.computer/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git`. That endpoint now answers "repository not found", so a checkout without a preloaded Cargo or Nix Git cache cannot acquire Campaign.

Campaign `4a669eadaeac50b48efd77383219372be122f692`, served by Campaign's public Radicle seed, changes five things a consumer sees:

- `FrontierState` is sealed: its fields are private and read through accessors.
- Each state identity chains the previous state identity and the applied control event identity, so control events and selections recorded under the earlier whole-state framing name state identities the new framing never derives.
- Campaign errors carry context and implement `Display` and `core::error::Error`.
- The default Campaign profile admits 32,768 candidates and selections instead of 65,536.
- Campaign is licensed `AGPL-3.0-or-later` from its revision `6dc0c59a`; `e7757a51` was `MIT OR Apache-2.0`. ChaosControl is `AGPL-3.0-or-later` too, but `deny.toml` admits AGPL only through explicit per-crate exceptions.

ChaosControl reads Campaign state fields directly and persists Campaign control events in its journal, so it cannot build against the new revision unchanged, and it is the only VM consumer that exercises Campaign's frontier policy.

## What Changes

- Pin `campaign-core`, `campaign-choregraph`, and the `campaign-conformance` dev-dependency in Cargo, and the Nix `campaign-src` input, to `4a669ead` from `https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git`; update `deny.toml` (the seed source, and explicit `AGPL-3.0-or-later` exceptions for `campaign-core` and `campaign-choregraph`), the dependency-policy controls, the exact-revision transport list, and `docs/campaign-source-acquisition.md`. The source-pin requirement gains the rule that a pin's endpoint must serve it. r[chaoscontrol.campaign_policy.source_pins.acquirable]
- Read Campaign state through the sealed accessors, so the adapter builds against the pinned revision; no ChaosControl code edits a Campaign state. r[chaoscontrol.campaign_policy.source_pins.valid]
- Version the journal epoch header as `chaoscontrol.campaign-genesis.v2`, and refuse an epoch recorded under the earlier framing (`chaoscontrol.campaign-genesis.v1`) at its genesis record with a typed `EarlierStateFraming` failure. r[chaoscontrol.campaign_policy.journal_framing]
- Keep ChaosControl's own control session, and measure the durable cost of one journal segment per control event as evidence for a later decision on Campaign's `ControlSession`.
- Re-run the Campaign conformance suite and the frontier tests, and run one adaptive KVM exploration smoke per frontier policy at the earlier and at the new pin against the same kernel and initrd; each pin's parity checker replays both pins' runs, so decision equality rests on deterministic trace replay, not on live runs repeating. r[chaoscontrol.campaign_policy.source_pins]

## Impact

- **Files**: `crates/chaoscontrol-explore/Cargo.toml`, `crates/chaoscontrol-explore/src/campaign_adapter/**`, `crates/chaoscontrol-explore/src/explorer/frontier_backend/**`, `Cargo.lock`, `flake.nix`, `flake.lock`, `.cargo/config.toml`, `deny.toml`, `tools/check-campaign-dependency-policy.rs`, `docs/campaign-source-acquisition.md`, `README.md`, and this change package.
- **Compatibility**: an output directory whose last journal epoch was written under the earlier pin cannot resume; a fresh output directory starts a new journal. The identities of control events, selections, operations, and the child moments and candidates framed from them change across the pins, but no ChaosControl decision depends on an identity: every rank vector ends with the unique insertion dimension `i64::MAX - entry_seq`, and both the exploration order and the retention rank order end with it, so candidate identity never breaks a tie. ChaosControl derives its own Campaign bounds from its run configuration, so the smaller default profile applies only to the conformance test's copy of the default profile.
- **Testing**: the focused `chaoscontrol-explore` library tests with the pinned Campaign conformance digests, strict Clippy, formatting, the dependency-policy and exact-revision checks, Cairn validation, the A/B KVM smoke with frontier parity receipts, and `nix flake check -L` where the host can run it.
- **Owner and capability**: ChaosControl maintainers own the adapter, the journal, and the source pins. ChaosControl gains a dependency it can acquire from a public endpoint, cheaper journal replay through Campaign's chained identity, and typed refusal of journals it can no longer replay.

## Out of Scope

- Adopting Campaign's `ControlSession`; this change records the measurement that decision needs.
- Migrating or rewriting journals recorded under the earlier framing.
- Campaign-side changes, including a staged multi-event commit.
- Restoring the Cargo transport repair for Campaign, or moving the Choregraph history source, which its current endpoint still serves.
