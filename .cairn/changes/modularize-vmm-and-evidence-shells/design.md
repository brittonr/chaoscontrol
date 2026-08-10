## Context

`vm.rs`, `controller.rs`, and `replay_readiness_surfaces.rs` each own several unrelated responsibilities. Existing tests are strong, so the migration must preserve behavior at every slice.

## Decisions

### 1. Ownership comes before movement

Create a module map for state ownership, invariants, inputs, outputs, effects, and tests. A type moves only after its owner and dependency direction are explicit.

### 2. Pure plans precede effects

VM and controller cores produce checked transition, snapshot, device, and fault plans. Evidence cores produce loaded-fact classifications and render models. Shells execute plans and collect observations.

### 3. Public compatibility is frozen

Existing public Rust paths may use temporary re-exports. Public JSON names, enum meanings, error classes, and receipt semantics do not change in this package.

### 4. Migration uses bounded slices

Move one ownership domain at a time. Record focused test and Clippy baselines before each core slice. Remove compatibility re-exports only after call sites migrate.

### 5. Unsafe ownership gets explicit review

Manual `Send` and `Sync`, timer threads, KVM handle lifetimes, and teardown paths receive owner comments, assertions, and focused positive and negative tests.

### 6. Architecture validation checks direction

A lightweight source rule rejects core imports of filesystem, environment, process, clock, output, and KVM shell effects. It also rejects evidence rendering inside decision cores.

## Risks

Large moves can hide behavior changes. Small commits, baseline tests, and compatibility assertions limit this risk. Re-export layers are temporary and must have removal tasks.
