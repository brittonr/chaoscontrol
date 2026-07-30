# Validation

Date: 2026-07-30

## Result

Accepted for sync and archive. The implementation separates fault selection, applicability, application, failure, and observation. It rejects unverifiable snapshots and recordings before mutation.

The result does not claim that selecting or applying a fault proves guest-visible impact. Exact replay proves agreement with the bounded recorded trace only.

## Focused checks

The following checks passed:

- `cargo test -p chaoscontrol-fault`
- `cargo test -p chaoscontrol-vmm --lib`
- `cargo test -p chaoscontrol-replay`
- `cargo check -p chaoscontrol-explore --all-targets`
- `cargo clippy -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay --all-targets -- -D warnings`
- `cargo fmt -p chaoscontrol-fault -p chaoscontrol-vmm -p chaoscontrol-replay -p chaoscontrol-explore -- --check`
- `cargo test --workspace`
- `git diff --check`

The focused replay suite passed 87 tests. The focused VMM library suite passed 465 tests and ignored 9 tests. The workspace test rail passed, including 14 VMM documentation tests with 9 ignored tests.

Positive tests cover supported fault variants and effect consumption. Negative tests cover unsupported capabilities, malformed transitions, bad targets and parameters, ledger capacity, snapshots, recordings, schedule provenance, random provenance, application failures, replay horizons, and checkpoint-prefix mismatches.

The adapter registry and path tests cover r[chaoscontrol.fault_outcomes.effect_reachability] and r[chaoscontrol.fault_outcomes.application]. Operation-hook tests cover r[chaoscontrol.fault_outcomes.observation]. Replay and consumer tests cover r[chaoscontrol.fault_outcomes.compatibility]. The complete conformance run covers r[chaoscontrol.fault_outcomes.validation].

## Lifecycle checks

The canonical policy was:

`/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`

The following Cairn checks passed before sync:

- repository validation
- proposal gate
- design gate
- tasks gate

Direct validation without `--policy` failed because this repository does not contain `cairn-policy/generated/cairn-policy.json`. The canonical-policy checks are authoritative for this lifecycle package.

Post-sync traceability found 16 referenced requirements out of 24 accepted requirements. All 16 `chaoscontrol.fault_outcomes.*` requirements have implementation or verification evidence. The repository-wide traceability and release-readiness verdicts remain false only because eight earlier `kernel_bundle_validation.*` requirements have no source markers.

## Known broad-rail failure

`cargo clippy --workspace --all-targets -- -D warnings` still fails in unchanged code at `crates/chaoscontrol-evidence/src/kernel_bundle_initrd.rs:815`. Clippy reports `drop_non_drop` for `drop(writer)`. Focused Clippy checks for all changed runtime crates pass.

The repository-wide Tiger Style rail also has pre-existing canonical-path findings. These unrelated findings are not represented as passing.

## Advisory review

VibeThinker advisory review was unavailable because the local request did not complete. The request was aborted. This advisory review is not acceptance evidence.
