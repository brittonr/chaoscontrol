# Verification: default fault injection for system runs

## Subject

- `595b2c7` is the original change, replayed onto `main` at `e540904`; `controller.rs` merged without conflict.
- `eaef511` gives exploration the setting and records it.
- `e0f263a` declares the product-scope intent.
- Every check below ran on the tree of `e0f263a`.

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

## Non-claims

- Default random injection does not show that a run reaches every failure path.
- The host watchdog timeouts are a host or VM residual risk. They reproduce with random injection off, and this change does not address them.
- The dogfood `run-config.json` projection does not carry the setting.
