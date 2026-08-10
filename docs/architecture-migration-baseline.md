# Architecture migration baseline

This baseline was recorded before the VMM, controller, and evidence module migration on 2026-08-09.

## Source shape

| Surface | Rust lines | Code lines | Public compatibility owner |
| --- | ---: | ---: | --- |
| `crates/chaoscontrol-vmm/src/vm.rs` | 4,895 | 4,099 | `chaoscontrol_vmm::vm` |
| `crates/chaoscontrol-vmm/src/controller.rs` | 4,975 | 4,330 | `chaoscontrol_vmm::controller` |
| `crates/chaoscontrol-evidence/src/replay_readiness_surfaces.rs` | 4,527 | 4,390 | `chaoscontrol_evidence` re-exports |

## Behavior and schema

The pre-migration baseline passed these commands:

```text
cargo test -p chaoscontrol-vmm --lib
cargo test -p chaoscontrol-evidence --lib
cargo clippy -p chaoscontrol-vmm -p chaoscontrol-evidence --all-targets -- -D warnings
cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .
```

The VMM library baseline had 475 passing tests and 9 ignored tests. The evidence library baseline had 100 passing tests and no ignored tests.

Public Rust paths, serialized field names, enum meanings, error classes, receipt summaries, and deterministic transition results are frozen for this migration. Existing JSON and Nickel fixtures remain the compatibility oracle.

## Unsafe ownership

The migrated VMM surface has one manual unsafe trait implementation:

```text
crates/chaoscontrol-vmm/src/vm.rs: unsafe impl Send for SendTimerId
```

`SendTimerId` owns only a POSIX timer identifier. The controller transfers it into a scoped timer thread and joins that thread before it reuses or destroys the timer. The migration must keep creation, transfer, cancellation, join, and deletion under one explicit owner.

The SDK has separate manual `Send` and `Sync` implementations. They are outside this migration and do not transfer authority to the VMM modules.

## Baseline claim

This record establishes only observed pre-migration tests, source shape, and public compatibility surfaces. It does not prove safety, behavior completeness, or absence of defects.
