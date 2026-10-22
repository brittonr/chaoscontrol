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
| Durable append cost | `cargo test -p chaoscontrol-explore --lib --release -- journal::tests::durable_append_cost_by_records_per_segment --ignored --nocapture` with `CHAOSCONTROL_APPEND_COST_DIR` on the repository filesystem, and a census of the smoke journal's records per segment | Mean, median, 90th-percentile, and maximum latency of 200 appends of 2,048-byte records for one and thirteen records per segment; the number of segments and control records in the smoke journal. |
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
- After the benchmark was resized from the clean census (constants and doc comment of the ignored benchmark only), the three checks ran again on the final tree: formatting clean; 410 passed, 2 ignored, tests finished in 47.75 s under a load average near 57; Clippy without findings.

### Durable append cost

Census of the clean A/B smoke's Campaign journals (decoded segment by segment; both pins have the same layout): one epoch of 12 segments holding 29 control records. The genesis segment holds the genesis, the root moment, 3 control records, and an entry; five selection segments hold one control record and a selection record each (about 3.4 KB); three settlement segments hold a moment, 4 control records, an entry, and a round (about 6.9 KB); one holds 2 control records and a round; the largest, `seg-00000008`, holds 3 moments, 6 control records, 3 entries, and a round in 13,531 bytes; the stop segment holds one control record in 931 bytes. Control record payloads range from 724 to 3,703 bytes (median 1,436) in the `6cfe25fd` run's journal, the figures the benchmark's doc comment cites, and from 709 to 3,714 bytes (median 1,427) in the `a154ddd` run's journal. An earlier run's journal (12 segments, 21 control records, largest segment 7 records) is superseded by this census.

Host filesystem: ZFS dataset `datapool/git` (`sync=standard`, `compression=lz4`, `recordsize=128K`, `logbias=latency`), load average 57 during the run. `CHAOSCONTROL_APPEND_COST_DIR=target/adopt/append-cost cargo test -p chaoscontrol-explore --lib --locked --offline --release -- journal::tests::durable_append_cost_by_records_per_segment --ignored --nocapture`, 200 appends of 2,048-byte records per case:

| Records per segment | Mean | Median | 90th percentile | Maximum |
| --- | --- | --- | --- | --- |
| 1 | 1.80 ms | 1.92 ms | 2.18 ms | 2.95 ms |
| 13 | 2.09 ms | 2.14 ms | 2.46 ms | 6.56 ms |

One segment per control event would publish 29 segments instead of 12 in this 5-round run, adding 17 appends, about 31 ms at the one-record mean, to a run whose rounds take seconds. The append cost grows little with the segment size up to the largest observed segment. Crash recovery of a split settlement is not measured.

### A/B KVM smoke

Evidence class: execution evidence for the recorded inputs only; the decision claim rests on deterministic trace replay.

- Explorers, built with `cargo build --release --offline --locked -p chaoscontrol-explore --bins` in private target directories from clean trees: `a154ddd` (a clean detached worktree; buildable only offline, because `e7757a51` is served only from a preserved Cargo Git cache) and `6cfe25fd42296f20411381569598e90431ec366a` (this change's committed code).

| Binary | BLAKE3 |
| --- | --- |
| `a154ddd` `chaoscontrol-explore` | `5828b5b2f5f8a8c0385d01c5c56e8e39a47d83fe8c381c0a0785f4bd0c1ac74c` |
| `a154ddd` `check-frontier-parity` | `66d6c74acfb5ed3d6e25537dc7fcc29fa119ec498694927ae95cdcf5915127bd` |
| `6cfe25fd` `chaoscontrol-explore` | `f5ca69cbb446c8e37787d84553a4bff3d62b242a47b89c06854fccfabebc25a5` |
| `6cfe25fd` `check-frontier-parity` | `97897177ef04f376294c43e1c97067cbbc8c9bfa42367ea2bb62d5a7cf52a120` |

- Kernel: `.#kcov-vmlinux` = `/nix/store/x9qp3ls75w73mxf1mvypsj6p8zmyk9x4-chaoscontrol-vmlinux`, the same store path as the 2026-09-26 smoke, rebuilt here; its `vmlinux` has the recorded BLAKE3 `3c0823c6c9480670479bdaf7916a57885c89315a560456c6191197ef7b469b7b`.
- Initrd: `.#initrd-rust-workload` built from the clean `6cfe25fd` tree = `/nix/store/lj4y2f812g9mi9fkznxljmaki676yc5f-chaoscontrol-initrd-rust-workload` (390,750 bytes, BLAKE3 `33adeba1974db86d1adadd2392decda3a5f352b8c8add79631bed349d4920fe7`).
- Each explorer ran `run --kernel <kernel>/vmlinux --initrd <initrd> --vms 1 --rounds 5 --branches 4 --ticks 500 --seed 42 --mode fault-schedule --memory-mb 128 --frontier-policy <policy>` with `RUST_LOG=info,chaoscontrol_explore=debug`, once with `legacy-diagnostic` and once with `campaign`, sequentially on 2026-10-21 between 21:52 and 21:59 EDT (load average 16 to 30). The runs went through the agent's background job runner rather than pueue, because a pueue daemon started from the agent shell does not survive the shell's process-group cleanup.

| Run | Exit | Selected entries per round | Explore draws | Pruned | Wall clock | `frontier-trace.jsonl` BLAKE3 |
| --- | --- | --- | --- | --- | --- | --- |
| `a154ddd` legacy-diagnostic | 0 | 0, 1, 1, 2, 3 | none | none | 16.6 s | `e62c3149e8d720088b3012c0a4ee14ff836ca10d42b514b237dce19868b3797d` |
| `a154ddd` campaign | 0 | 0, 1, 1, 2, 3 | none | none | 17.1 s | `9d195abfefd1560ac225a7d17feecf472e3b1af079c1ff00373fe5623b004eab` |
| `6cfe25fd` legacy-diagnostic | 0 | 0, 1, 1, 1, 1 | none | none | 5 min 41 s | `106c5373c6989e34ba8125f9ccc826c3267a9a999c14de5413ec36e5ca0de6a3` |
| `6cfe25fd` campaign | 0 | 0, 1, 2, 1, 3 | none | none | 16.3 s | `2cb7098995b219a5af1376a90de17b3bf2f1d33c89aab8bdc27d5f7350b3e968` |

Parity, each run pair checked by each pin's `check-frontier-parity` (every check printed `legacy-run-replayed-through-campaign: 5 rounds, decision fields equal, semantic work Measured { equal: true }`, the same line for `campaign-run-replayed-through-legacy`, and `frontier parity ok`, exit 0):

| Checker | Run pair | Receipt BLAKE3 |
| --- | --- | --- |
| `a154ddd` | `a154ddd` | `a0c4c7ea2d0792006b59d0e539a219567b92b0b38a77b4fa5f15558e371dfec9` |
| `6cfe25fd` | `a154ddd` | `a0c4c7ea2d0792006b59d0e539a219567b92b0b38a77b4fa5f15558e371dfec9` |
| `6cfe25fd` | `6cfe25fd` | `687d7bce4656620caa0e83b5e91fcb91bc1f2be9ccdce3f521b7df0e87440e5f` |
| `a154ddd` | `6cfe25fd` | `687d7bce4656620caa0e83b5e91fcb91bc1f2be9ccdce3f521b7df0e87440e5f` |

Each run pair's receipt is byte-identical under both checkers. Replay drives only the policy a run is replayed through, so the Campaign backends run only on the legacy-run observations: on each pin's legacy-run observations and draws, the `e7757a51` and `4a669ead` Campaign backends make the legacy policy's decisions, with the same semantic planning work, and so each other's. The campaign-run direction exercises the legacy policy, identical at both pins. The two distinct receipts are committed as `evidence/kvm-smoke-2026-10-21-a154ddd-frontier-parity-receipt.json` (`a0c4c7ea…`) and `evidence/kvm-smoke-2026-10-21-6cfe25fd-frontier-parity-receipt.json` (`687d7bce…`); traces, journals, and run logs stay local.

Observations, not claims:

- The live traces differ across the pins and also between the two legacy-diagnostic runs, which run identical code, so live exploration does not repeat on this host, as the 2026-09-25 record found.
- Some runs spend minutes in round 1: the restored guest stops making SDK calls, and each branch runs until the 50,000-exit idle detector halts it, with the explorer mostly blocked (16 % CPU and about 14,000 voluntary context switches, against 95 % and about 100 in a fast run). Across seven runs of the `6cfe25fd` explorer this happened six times, and across the first seven runs of the `a154ddd` explorer never. It is not caused by this change's Campaign code: the slow `6cfe25fd` run in the table above is a legacy-diagnostic run, which runs none of the changed Campaign code, and a control reproduced the slowdown with the unchanged `a154ddd` explorer (BLAKE3 `5828b5b2…`, 10 min 16 s, 8 idle branches) and with an `a154ddd` explorer rebuilt with four codegen units (BLAKE3 `cdec72893a7cac84f883c490edb67680235c49aefe6c8864120870e50c3ec7ba`, 29 min 13 s with 4 idle branches; a second run of that binary took 18.5 s). Why its rate varies with the explorer binary, and why it was higher at `6cfe25fd`, is not explained; `risk.md` records it as a residual risk. Two of the `6cfe25fd` runs belong to an earlier session whose explorer, built from an uncommitted tree, has the same release binary hash; that session's own-pin and cross-pin parity checks also passed.
