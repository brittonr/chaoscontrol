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

`SendTimerId` owns only a POSIX timer identifier. The controller transfers it into a scoped timer thread and joins that thread before it reuses or destroys the timer. After migration, `crates/chaoscontrol-vmm/src/unsafe_owner.rs` is the explicit owner of transfer and deletion.

The SDK has separate manual `Send` and `Sync` implementations. They are outside this migration and do not transfer authority to the VMM modules.

## Baseline claim

This record establishes only observed pre-migration tests, source shape, and public compatibility surfaces. It does not prove safety, behavior completeness, or absence of defects.

## Post-migration comparison

The migration moved owned logic into `vm_core`, `controller_core`, the replay-readiness owner modules, and `unsafe_owner`.

| Former shell | Rust lines after migration | Code lines after migration | Code-line reduction |
| --- | ---: | ---: | ---: |
| `vm.rs` | 4,849 | 4,069 | 30 |
| `controller.rs` | 4,856 | 4,224 | 106 |
| `replay_readiness_surfaces.rs` | 4,369 | 4,242 | 148 |

Post-migration focused results:

- VMM library: 486 passed, 9 ignored.
- Evidence library: 113 passed.
- Architecture boundary checker: 4 pure cores passed; `unsafe_owner.rs` was the only manual unsafe-trait owner.
- Focused and workspace Clippy passed with warnings denied.
- Workspace tests, including admitted KVM scheduling and snapshot tests, passed.
- The focused Nix evidence-contract and Tiger Style checks passed.
- Full `nix flake check -L` reached the unrelated SpaceWasm rail and stopped on its pinned Mantle manifest mismatch. The same focused SpaceWasm check fails on unchanged `origin/main` with expected `4ff6a779...` and actual `39e4790a...`.

The line counts show ownership movement only. They do not measure complexity, safety, or correctness.
