## Phase 1: Baseline and source pins

- [x] [serial] r[chaoscontrol.campaign_policy.source_pins.valid] Record the focused baseline at `a154ddd`: `cargo test -p chaoscontrol-explore --lib --locked --offline` and strict Clippy on the library and its tests. See `verification.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.source_pins.acquirable] Pin `campaign-core`, `campaign-choregraph`, `campaign-conformance`, and the Nix `campaign-src` input to `4a669eadaeac50b48efd77383219372be122f692` from Campaign's public seed, refresh `Cargo.lock` and `flake.lock` from the live seed, and update `deny.toml` (the seed source and per-crate `AGPL-3.0-or-later` exceptions for `campaign-core` and `campaign-choregraph`), `tools/check-campaign-dependency-policy.rs`, `.cargo/config.toml`, and `docs/campaign-source-acquisition.md`. See `verification.md`.

## Phase 2: Adapter and journal

- [x] [serial] r[chaoscontrol.campaign_policy.source_pins.valid] Replace every read of a private `FrontierState` field with its accessor. See `verification.md`.
- [x] [serial] r[chaoscontrol.campaign_policy.journal_framing] Version the genesis schema as `chaoscontrol.campaign-genesis.v2`, refuse `chaoscontrol.campaign-genesis.v1` with `EarlierStateFraming` and other schemas with `UnknownSchema`, pin the journal's state identities in a test, and document the refusal in `README.md`. See `verification.md`.

## Phase 3: Verification

- [x] [serial] r[chaoscontrol.campaign_policy.journal_framing.invalid] Negative: an epoch whose genesis names the earlier framing or an unknown schema fails recovery at its genesis record, and recovery writes no segment or epoch. Positive: an epoch with the current schema recovers, closes, and starts the next epoch.
- [x] [serial] r[chaoscontrol.campaign_policy.source_pins.valid] Run the `chaoscontrol-explore` library tests, including the Campaign conformance suite with 80 fixtures and both pinned transcript digests, and compare the test list with the baseline.
- [x] [serial] r[chaoscontrol.campaign_policy.source_pins] Run the A/B KVM smoke: one exploration per frontier policy with clean-tree release explorers at `a154ddd` and at this change's committed head, against one kernel and initrd; check each pin's run pair with both pins' `check-frontier-parity`, and record trace digests and the legacy-diagnostic repeatability control as observations.
- [x] [depends:chaoscontrol.campaign_policy.source_pins] Evidence for the later `ControlSession` decision, which needs the pinned revision's journal: count the records per segment in the smoke's Campaign journal, and run the ignored benchmark `durable_append_cost_by_records_per_segment` (200 appends of 2,048-byte records for one record per segment and for the largest observed segment) on the host filesystem.
- [ ] [serial] r[chaoscontrol.campaign_policy.source_pins] Run formatting, strict Clippy, the dependency-policy controls, the exact-revision transport check, Cairn validation and gates, and `nix flake check -L` where the host can run it, and record the results in `verification.md`.
- [ ] [serial] r[chaoscontrol.campaign_policy.journal_framing] After the delta spec is synchronized, run Cairn validation, the proposal, design, and tasks gates, and traceability, and record that `journal_framing` and its scenarios and `source_pins.invalid` and `source_pins.acquirable` are referenced, while `source_pins` and `source_pins.valid` stay missing as before this change because their declarations live in Cargo, Nix, and TOML files that the marker scan does not read.
