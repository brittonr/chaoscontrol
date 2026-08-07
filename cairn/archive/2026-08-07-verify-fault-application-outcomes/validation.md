# Validation

Date: 2026-08-07

## Result

The fault-stage implementation now works with the accepted assertion-identity boundary on current `origin/main`.

The restore path validates fault stages and assertion state before mutation. Controller-only snapshots use the explicit orchestration restore path.

## Baseline

The current `origin/main` baseline passed 562 focused library tests. Nine KVM tests were ignored.

Command:

```console
cargo test -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay --lib
```

## Post-merge checks

The following commands passed:

```console
cargo check -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay -p chaoscontrol-explore --all-targets
cargo test -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay --lib
cargo clippy -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay --all-targets -- -D warnings
```

The post-merge test rail passed 643 tests. Nine KVM tests were ignored.

Positive tests cover valid fault stages, supported adapters, effect observations, replay, and snapshot continuation.

Negative tests cover invalid transitions, targets, parameters, bounds, unsupported capabilities, malformed snapshots, and assertion-state conflicts.

## Lifecycle checks

The canonical policy was:

`/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`

Repository validation and the proposal, design, and tasks gates passed.

The sync dry-run reported all ten requirements as `already_applied`. The accepted-spec hash did not change.

Archive execution succeeded at:

`cairn/archive/2026-08-07-verify-fault-application-outcomes`

Post-archive validation passed with ten active changes.

## Known broad-format difference

`cargo fmt --all -- --check` reports an unchanged format difference in `crates/chaoscontrol-explore/src/server.rs`.

The focused format command completed for `chaoscontrol-fault`, `chaoscontrol-vmm`, and `chaoscontrol-replay`.
