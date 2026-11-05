# VMM collection evidence

Date: 2026-11-05. Source: `main` worktree on base `bcfc3b17` plus this change. Host toolchain Cargo was used directly; results are recorded as run.

## Scope

This evidence covers the VMM shell slice: bounded per-VM retention of opaque protocol-observation envelopes at the lifecycle hypercall, deterministic scheduler-position binding, producer-scheduler-claim rejection, fail-closed retention bounds, restore-time clearing, and the SDK producer sequence origin.

## Changes

- `crates/chaoscontrol-vmm/src/protocol_observation.rs` publishes `ProtocolObservationCollector`, `ProtocolCollectionLimits`, `ProtocolCollectionRejections`, `ProtocolObservationState`, `collect_hypercall`, and `scheduler_position_ref`. Default retention limits mirror the published contract ceilings (`MAX_RECORDS`, `MAX_BYTES`).
- `DeterministicVm` collects a payload only when the oracle admits the lifecycle event for an active run, binds the vCPU scheduler state identity, exposes `take_protocol_observations` / `protocol_observation_state`, and clears retained observations on full and incremental restore.
- `chaoscontrol-sdk` starts source sequences at 1 so emitted envelopes satisfy the cohort contract's contiguous 1-based sequence rule.

## Checks

| Command | Result |
| --- | --- |
| `cargo test -p chaoscontrol-vmm --lib protocol_observation` | exit 0; 9 passed, 0 failed |
| `cargo test -p chaoscontrol-vmm --lib` | exit 0; 580 passed, 0 failed, 9 ignored |
| `cargo test -p chaoscontrol-sdk --lib protocol_observation` | exit 0; 2 passed, 0 failed |
| `cargo test -p chaoscontrol-smr --test protocol-observation` | exit 0; 16 passed, 0 failed |
| `cargo check --workspace --all-targets` | exit 0 |
| `cargo clippy -p chaoscontrol-vmm -p chaoscontrol-sdk --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cairn validate --root .` | exit 0; `valid: true`, 0 issues, 82 changes, 175 specs validated |
| `cargo octet check -p chaoscontrol-vmm` | exit 0; `warning-only`, 0 errors, 588 warnings (existing lint families) |
| `cargo octet check -p chaoscontrol-sdk` | exit 1; 2 `ambient_clock` errors in untouched `crates/chaoscontrol-sdk/src/diagnostic.rs` |
| `nix build .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused` | not completed; Nix auto-GC lock contention and a build-hook lock-name failure in the host environment |

The scoped VMM check reports no error-class finding for this change. The SDK-scoped
errors are pre-existing on `HEAD` in an untouched file. The focused Nix check
currently aggregates those same pre-existing errors, so the repository-wide
policy gate is red independently of this slice.

The collector tests cover: opaque projection retention with a VMM-bound scheduler ref; missing scheduler slot binding; producer scheduler claims; unknown fields; non-object and non-JSON payloads; record and byte bound overflow without unbounded growth; drain and reset; canonical scheduler references; hypercall payload round-trip; unrelated lifecycle event ignoring; and the SDK producer envelope parsing into the VMM record with sequence 1.

## Boundaries

No KVM campaign ran. The guest workload adoption that emits these envelopes belongs to the consumer adapters and is not evidenced here. Collection proves transport shape, scheduler-position binding, and bounded retention only; it does not prove protocol correctness, cohort completeness, replay, or release eligibility.
