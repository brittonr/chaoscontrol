# Validation

Date: 2026-08-07

## Result

The deterministic SMP implementation now works with the accepted assertion, fault, and virtio boundaries on current `origin/main`.

Host interrupts remain operational watchdog inputs. They do not select a vCPU or advance deterministic schedule state. VM and controller poison prevent partial progress from becoming replay authority.

## Post-merge checks

The following commands passed:

```console
cargo fmt -p chaoscontrol-vmm -p chaoscontrol-fault -p chaoscontrol-replay
cargo check -p chaoscontrol-vmm -p chaoscontrol-fault -p chaoscontrol-replay -p chaoscontrol-explore --all-targets
cargo test -p chaoscontrol-vmm --lib
cargo test -p chaoscontrol-vmm --test prop_scheduler
cargo test -p chaoscontrol-vmm --test deterministic_smp_kvm
cargo clippy -p chaoscontrol-vmm -p chaoscontrol-fault -p chaoscontrol-replay --all-targets -- -D warnings
```

The VMM library rail passed 484 tests. Nine KVM tests were ignored. The scheduler and deterministic SMP KVM rails passed.

Positive tests cover deterministic transition progress, exact boundaries, snapshot continuation, PMU operation, and controller rounds.

Negative tests cover invalid vCPUs, stale events, progress overshoot, unavailable PMU support, post-commit failures, malformed snapshots, and partial-round failure.

## Lifecycle checks

The canonical policy was:

`/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`

Repository validation and the proposal, design, and tasks gates passed.

The sync dry-run reported all ten requirements as `already_applied`. Sync execution did not change the accepted specification.

Archive execution succeeded at:

`cairn/archive/2026-08-07-remove-wall-clock-smp-preemption`

Post-archive validation passed with nine active changes.

## Claim boundary

These checks prove the tested deterministic scheduling, poison, snapshot, and evidence behavior. They do not prove arbitrary host scheduling, KVM, PMU hardware, or guest correctness.
