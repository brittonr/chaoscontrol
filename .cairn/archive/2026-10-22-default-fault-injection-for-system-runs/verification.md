# Verification: default fault injection for system runs

## Subject

- `595b2c7` is the original change, replayed onto `main` at `e540904`; `controller.rs` merged without conflict.
- `eaef511` gives exploration the setting and records it.
- `e0f263a` declares the product-scope intent.
- `ce75182` carries the recorded setting into the dogfood `run-config.json` projection.
- The repository, Nix, and end-to-end checks below ran on the tree of `e0f263a`. The run-config projection and lifecycle checks ran on the tree of `ce75182`.

The original change did not build `chaoscontrol-explore` or `chaoscontrol-replay`. Five `SimulationConfig` literals named every field and lacked `random_faults`: in `explorer.rs`, `worker.rs`, `minimizer.rs`, the explorer binary's `reproduce`, and `replay.rs`.

## Repository checks

- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: no findings.
- `cargo test` for `chaoscontrol-explore`, `chaoscontrol-replay`, `chaoscontrol-evidence`, and `chaoscontrol-vmm`: every suite passes.
  - `chaoscontrol-explore` library: 425 passed, 2 ignored.
  - `chaoscontrol-vmm` library: 527 passed, 9 ignored.
- `check-evidence-contracts --root .`: pass, including the new positive and negative Nickel fixtures for the run profile and the bug report.
- `check-profile-projections --root .`: pass after the trusted `run-config.ncl` identity moved to `blake3:df97c62b…`. Only the contract identity in the run and campaign projection receipts changed.
- `check-product-scope --root .`: pass.
- `cairn validate --root . --strict`: valid, with no issues or findings, while this package is active.

## Nix and KVM

`nix flake check -L --keep-going --option builders ''` evaluated every check.

- Every check passed except `local-multi-hypervisor-kvm-smoke`. In that check, both of its runs ended in `HostWatchdogTimeout { vcpu: 0 }`.
- An interleaved A/B of `raft-accepted-verdict-dogfood` showed that this timeout does not depend on the setting.
  - Round 1: both runs passed, with the setting off and with the default.
  - Round 2: both runs stopped with the same timeout within 26 seconds, again with the setting off and with the default.
- A rebuild of `checks.x86_64-linux.local-multi-hypervisor-kvm-smoke` on the same tree passed both runs (`status=recorded runs=2 passed=2`).
- `snapshot-replay-smoke` passed with the default on. It exported bug 1 for assertion 3463273124 at replay depth 1, and the replay verdict artifact passed.

## End-to-end record and reproduction

The raft accepted-verdict rail ran twice on KVM.

| Run | `checkpoint.json` | `bug_1.json` | `reproduce` | Verdict |
| --- | --- | --- | --- | --- |
| Default | `random_faults: true` | `random_faults: true` | "Random faults: true (recorded with the bug)" | accepted snapshot-backed |
| `--random-faults false` passed to `run` only | `random_faults: false` | `random_faults: false` | "Random faults: false (recorded with the bug)" | accepted snapshot-backed |

`reproduce` received no setting in either run, so it used the value recorded in the bug.

## Run-config projection

- `recorded_random_faults_is_carried_and_legacy_checkpoints_keep_the_shape` passes. A checkpoint without the setting keeps the legacy run-config shape, both recorded values round-trip through the compatibility encoding and validate, and a string, number, or null value is refused.
- `check-evidence-contracts --root .` passes with the new valid and invalid run-config fixtures. The whole `chaoscontrol-evidence` test suite and its Clippy pass.
- `check-contract-registry .` passes: 21 families. The run-profile, run-config, and bug-report fixtures for the setting are registered.

## Lifecycle

- `cairn gate proposal`, `design`, and `tasks` each return `PASS` with no issues. Their receipts are `ede1a85c7d81c440b638d3c7edd10bda97943c173a7f5c079a82a686e031dfd7`, `c61956a5272bd9c1041656064898ea96741a025994e8d6cbc9f48bef7969f056`, and `f1b305cf71c17d80eec93060521445a5d533c5078c4ce8ecff01ee0437d15ffc`.
- `cairn sync --execute` merged the delta into `.cairn/specs/system-run-fault-injection/spec.md` (receipt `d8affe86a3708727af91520f8cd2db4ea72438989930ff59dbe8be8cc09df883`).
- `cairn validate --root . --strict` is valid with no issues or findings after the sync.
- `cairn archive --execute` with `CAIRN_ARCHIVE_DATE=2026-10-22` moved the package to `.cairn/archive/2026-10-22-default-fault-injection-for-system-runs` (receipt `357780be06a6f434e36fda34c50c3808e0543a39a009c7da58577956b22abdf2`).

## Final gate on `bbecdb8`

`nix flake check -L --keep-going --option builders ''` ran on the tree of the archive commit `bbecdb8`.

- Every check passed except `local-multi-hypervisor-kvm-smoke`. `snapshot-replay-smoke` passed.
- While that smoke ran, unrelated host work held the load average between 51 and 66 on 32 cores. A local run of the same smoke ended both workloads in `HostWatchdogTimeout { vcpu: 0 }`, and three sandbox rebuilds also failed (1 of 2, 1 of 2, and 0 of 2 runs passed).
- An interleaved raft-rail A/B on the same tree, at load 12 to 40, failed one run with random injection off on the same timeout. Both default-on runs and the second opt-out run passed.
- After the load fell to about 8, a rebuild of `checks.x86_64-linux.local-multi-hypervisor-kvm-smoke` passed both runs (`status=recorded runs=2 passed=2`).

## Non-claims

- Default random injection does not show that a run reaches every failure path.
- The host watchdog timeouts are a host or VM residual risk. They reproduce with random injection off, and this change does not address them.
