# Verification plan for adopt-campaign-chained-identity

Planned checks come first; observed results are added under "Observed results" only after the commands ran.

## Positive checks

| Check | Command | Expected evidence |
| --- | --- | --- |
| Library tests | `nix develop -c cargo test -p chaoscontrol-explore --lib --locked --offline` | All pass, including the Campaign conformance suite with 80 fixtures and the pinned transcript digests on the default profile of Campaign `4a669ead` (32,768 candidates and selections); the test list equals the baseline's plus the new journal tests. |
| Lints | `cargo fmt --all -- --check`; `cargo clippy -p chaoscontrol-explore --lib --tests --locked -- -D warnings` | No findings. |
| Dependency policy | `nix build .#checks.x86_64-linux.dependency-policy`; `nix build .#checks.x86_64-linux.cargo-exact-revisions` | Pass with the seed URL admitted. |
| Fresh acquisition | `cargo fetch --locked` with a new, empty `CARGO_HOME`; `nix flake prefetch --refresh` of the `campaign-src` URL | Cargo fetches `4a669ead` from the seed without changing `Cargo.lock`, with the same Git tree as Campaign's own `4a669ead`; Nix's refreshed fetch has the `flake.lock` narHash. |
| Journal state identities | `the_genesis_schema_names_the_pinned_state_identities` | The pinned identities hold at this change, and the same test at `a154ddd` yields other identities. |
| A/B KVM smoke | Release explorers built from clean trees at `a154ddd` and at this change's committed head; one bounded run per frontier policy per explorer against one kernel and one initrd built from a committed tree; each pin's `check-frontier-parity` over both pins' run pairs | Every run exits 0, and all four parity receipts report every decision field equal. Trace digests across the pins and the legacy-diagnostic repeatability control are observations only. |
| Durable append cost | `cargo test -p chaoscontrol-explore --lib --release -- journal::tests::durable_append_cost_by_records_per_segment --ignored --nocapture` with `CHAOSCONTROL_APPEND_COST_DIR` on the repository filesystem, and a census of the smoke journal's records per segment | Mean, median, 90th-percentile, and maximum latency of 200 appends of 512-byte records for one and seven records per segment; the number of segments and control records in the smoke journal. |
| Cairn | `cairn validate`, `cairn gate proposal|design|tasks`, traceability after sync | Pass; `journal_framing` and its scenarios, `source_pins.invalid`, and `source_pins.acquirable` are referenced. |
| Full gate | `nix flake check -L` | Pass, or each check that cannot run on this host is named. |

## Negative checks

| Check | Expected evidence |
| --- | --- |
| Earlier-framing journal | An epoch whose genesis names `chaoscontrol.campaign-genesis.v1` fails recovery with `EarlierStateFraming` at record 0, and recovery writes no segment or epoch. |
| Unknown journal schema | An epoch whose genesis names another schema fails with `UnknownSchema` the same way. |
| Unadmitted Campaign source | The dependency-policy controls replace the seed URL in `deny.toml` and `cargo deny` reports `source-not-allowed`. |
| Unadmitted Campaign license | The dependency-policy controls rename the `campaign-core` and `campaign-choregraph` license exceptions and `cargo deny` reports `failed to satisfy license requirements`. |

## Observed results

Host: AMD Ryzen 9 9950X3D (16 cores, 32 threads), 186 GiB RAM, Linux 6.18.50, `/dev/kvm` world-writable, shared with other users' builds (load average 15 to 25). Toolchain from the ChaosControl development shell: Cargo 1.98.0 with the exact-revision repair, rustc 1.98.0. Runs on 2026-10-21 in a worktree of `a154ddd1320129a133bb84b8e13ad3df7e55a55f` on branch `cairn/adopt-campaign-4a669ead-20261021`; the main checkout's uncommitted files were not touched.

### Baseline at `a154ddd`

- `cargo test -p chaoscontrol-explore --lib --locked --offline`: 408 passed, 1 ignored (409 listed tests), tests finished in 84.75 s.
- `cargo clippy -p chaoscontrol-explore --lib --tests --locked --offline -- -D warnings`: no findings.

### Source pins

- `git ls-remote https://git.onix.computer/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git` answers `repository ... not found`. `git ls-remote https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git` lists `HEAD` and `refs/heads/main` at `4a669eadaeac50b48efd77383219372be122f692`. The Choregraph endpoint `https://git.onix.computer/zL2ncTUeASVYwcoGkEXv9JKgGbAF.git` lists `HEAD` and `refs/heads/main` at `b3e08e19750f53bdbcae970cdf58a47a791ed20b`.
- `cargo update -p campaign-core -p campaign-choregraph -p campaign-conformance` fetched `main` and `HEAD` from the seed and changed only the three Campaign packages in `Cargo.lock`; the other 117 dependencies stayed.
- `nix flake update campaign-src` locked `4a669ead` from the seed (narHash `sha256-L468HoGcG1ZkOvYsJcjN9LBfKxL4HEEq6I5tBstp+fs=`, revCount 112), and the `flake.nix` assertion `campaign-src.rev == campaignRevision` names the same revision.

### Adapter migration

- Against `4a669ead`, `cargo check -p chaoscontrol-explore --all-targets --locked --offline` failed only with `E0616` (private field) on 67 reads of `FrontierState` fields in 16 files: `stopped`, `outstanding`, `selection_count`, `state_id`, `candidates`, `admission`, `reserved_resources`, `findings`, `accepted_results`, and `settled`. Each read became the accessor of the same name; reads of the stop decision use `stopped()` (`Option<&StopDecision>`), with `.cloned()` where a test owned the value. The check then passed with no other error.
- `cargo test -p chaoscontrol-explore --lib --locked --offline` on the migrated tree before the journal change: 408 passed, 1 ignored, the same 409 listed tests as the baseline, tests finished in 18.72 s. The Campaign conformance tests pass with `PINNED_FIXTURE_COUNT = 80` and the digests `28b873e9873c97b5b57194bbb499503e05fae0dc16633b9fe0ce844e0328b51b` and `37b9ff95d8c8d5b1b175be8e3e8445261d68daadddb3d903530b0186976c5bae`, unchanged.

### Dependency policy

- The first `nix build .#checks.x86_64-linux.dependency-policy` after the pin move failed with `licenses FAILED`: `campaign-core` and `campaign-choregraph` declare `license = "AGPL-3.0-or-later"` (Campaign relicensed in `6dc0c59a`), which the allow list admits only through per-crate exceptions. `sources ok` already held with the seed URL.
- With exceptions for `campaign-core` and `campaign-choregraph` the check reported `bans ok, licenses ok, sources ok` and `Campaign dependency-policy negative controls passed`. An exception for `campaign-conformance` was reported `license-exception-not-encountered` (the license check skips dev-dependencies) and its renaming control was accepted, so it was removed.
- `nix build .#checks.x86_64-linux.cargo-exact-revisions` passes.

### Fresh acquisition

- `cargo fetch --locked` with a new, empty `CARGO_HOME` (`target/adopt/fresh-cargo-home`, run in the development shell so `.cargo/config.toml` applied) exited 0 and fetched from `https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git`; `--locked` left `Cargo.lock` unchanged. The fetched Git database resolves `4a669ead` to tree `ef05d21c46e002567297600fef3dd3cfecb9a539`, the same tree as Campaign's own checkout of that revision, so the fetched source is byte-identical. The Choregraph history (`git.onix.computer`) and the other seed and Radicle sources fetched in the same run.
- `nix flake prefetch --refresh --json 'git+https://seed.radicle.garden/z2scC9MCm3pxk9mX4FEidRKabQ5LN.git?rev=4a669eadaeac50b48efd77383219372be122f692'` returned narHash `sha256-L468HoGcG1ZkOvYsJcjN9LBfKxL4HEEq6I5tBstp+fs=`, equal to `flake.lock`.

### Journal framing

- `an_epoch_recorded_before_the_chained_state_identity_fails_recovery_at_its_genesis`: an epoch whose only segment holds a genesis naming `chaoscontrol.campaign-genesis.v1` fails `CampaignFrontier::open_with_io` with `FrontierError::Replay` at epoch 0, record 0, `EarlierStateFraming`; a genesis naming `chaoscontrol.campaign-genesis.v0` fails the same way with `UnknownSchema`; in both cases epoch 0 keeps its segment count and no epoch 1 directory appears. At `a154ddd` a `v1` genesis was the current schema, so that code accepts it and reports `OutOfOrder` for the missing root moment instead (read from `replay_epoch`, not run).
- `recovery_closes_the_outstanding_selection_and_starts_a_new_epoch` (now also marked `r[verify chaoscontrol.campaign_policy.journal_framing.valid]`) recovers a `v2` epoch, closes its outstanding selection, and starts epoch 1.
- `the_genesis_schema_names_the_pinned_state_identities` pins the state identity after the genesis segment, `06e4a5cb6cd1e1fbc240629b14d471b4776601e963e1c9097c4a1e9a7b117dbb`, and after the first published selection, `619708f8dd7a42266714a64d7e496bc823f71ad1a7b9df181eb0abdeea744eec`, of one fixed journal (seed 7, the test configuration, scripted publication). The same test grafted onto `a154ddd` (reading the `state_id` field) yields `8a2a69565543a53498e3c9e0225af331afc7b4d3c63c3d87bb2fb052a9a19186` and `9fbd1028c424c829d3e14db3dd3c260183d2b50f572c26a465cbe7d6d5ae8434`, so it would have failed on this re-pin; the graft was reverted.

### Final focused checks

On the tree with every code change, in a private target directory:

- `cargo fmt --all -- --check`: clean.
- `cargo test -p chaoscontrol-explore --lib --locked --offline`: 410 passed, 2 ignored (412 listed tests: the baseline's 409, the two journal tests, and the ignored append benchmark), tests finished in 31.53 s under a load average near 39.
- `cargo clippy -p chaoscontrol-explore --lib --tests --locked --offline -- -D warnings`: no findings.
- `cargo check --workspace --all-targets --locked --offline` (before the journal tests): passed with no warnings.

### Durable append cost

Host filesystem: ZFS dataset `datapool/git` (`sync=standard`, `compression=lz4`, `recordsize=128K`, `logbias=latency`), load average 39 during the run. `CHAOSCONTROL_APPEND_COST_DIR=target/adopt/append-cost cargo test -p chaoscontrol-explore --lib --locked --offline --release -- journal::tests::durable_append_cost_by_records_per_segment --ignored --nocapture`, 200 appends of 512-byte records per case:

| Records per segment | Mean | Median | 90th percentile | Maximum |
| --- | --- | --- | --- | --- |
| 1 | 1.92 ms | 2.04 ms | 2.28 ms | 3.34 ms |
| 7 | 1.84 ms | 2.03 ms | 2.19 ms | 3.09 ms |

A throwaway census of the Campaign journal of the KVM smoke's Campaign run (5 rounds; recorded with an explorer built before the final commit, read from a copy) found one epoch of 12 segments holding 21 control records: the genesis segment (genesis, root moment, 3 control records, entry), five selection segments of one control record each, the first settlement segment (moment, 4 control records, entry, round) with 7 records, four settlement segments of 2 control records each, and a stop segment of 1. One segment per control event would therefore publish 21 segments instead of 12, adding 9 appends, about 17 ms at the measured mean, to a run whose rounds take seconds. The per-append cost does not depend on the record count up to seven; crash recovery of a split settlement is not measured.
