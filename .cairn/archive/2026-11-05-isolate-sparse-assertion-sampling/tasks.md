## Tasks

- [x] [serial] Record baseline sparse behavior, concrete consumers and the bounded design. r[chaoscontrol.sparse_sampling.identity]
- [x] [serial] Reproduce cross-namespace alias suppression, then sample only accepted fingerprints across both emission paths. r[chaoscontrol.sparse_sampling.identity]
- [x] [serial] Exercise first-pass/first-fail, declarations and default-mode behavior with local output and feature-matrix checks; record evidence and update parity documentation. r[chaoscontrol.sparse_sampling.outcomes]

## Evidence (2026-10-28)

- Source baseline: `3ac477f6` plus this change. Maintenance owner: ChaosControl SDK maintainers. Immediate outcome: no lost first outcomes for distinct accepted identities. Durable capability: collision-safe, bounded per-identity sparse instrumentation, regression-tested through both call paths.
- Baseline `cargo test -p chaoscontrol-sdk --features sparse_emission --test sparse_emission`: passed (1). New `sampling_identity` regression failed before the fix with `sampling right/true outcomes: left 0, right 1`, using admitted cross-namespace alias collision `2418279297`.
- After the fix: `cargo test -p chaoscontrol-sdk --test sampling_identity` passed (1); `cargo test -p chaoscontrol-sdk --features sparse_emission --test sampling_identity --test sparse_emission` passed (2). Checks include true/false first outcomes, repeated suppression, mixed stable/compatibility calls, unresolved-call isolation, declarations and default per-hit counts.
- `cargo test -p chaoscontrol-sdk --all-features`: 115 passed, 21 ignored; `cargo test -p chaoscontrol-sdk`: 112 passed, 21 ignored. `cargo test -p chaoscontrol-sdk --no-default-features --test bundle_assertions_no_std`: 8 passed (existing warnings reported).
- Throwaway `cargo run -q -p chaoscontrol-sdk --features sparse_emission --example sparse_identity_smoke` printed one true and one false observation for each of two distinct accepted fingerprints after four evaluations per site. Its local output and example source were removed after execution.
- `cargo check --workspace --all-targets`, `cargo clippy -p chaoscontrol-sdk --all-targets --all-features -- -D warnings`, and `cargo fmt --all -- --check`: passed.
- Product-scope projection regenerated from Nickel. `check-product-scope --root . --write` and `cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .`: passed.
- Cargo commands used `TMPDIR=/home/brittonr/.cargo-target/tmp`. kache reported storage I/O failures and compiled uncached; checks succeeded.
- This is local SDK instrumentation evidence, not KVM execution, replay, hosted-platform parity or release eligibility. The prior global lifecycle blocker at `add-native-process-record-replay-profile/tasks.md:17` is outside this change; global validation was not repeated just to confirm it.
- Final proposal/design/tasks gates passed. Native sync installed the two implemented requirements while leaving this change active. Traceability recognizes 398 requirements with none missing and both new sampling IDs linked; its overall verdict remains failed on the same nine unrelated native-process-profile/SMR-storage-recovery dangling IDs.
