## Tasks

- [x] [serial] Record the existing JSON guidance baseline and bounded design. r[chaoscontrol.stable_json_guidance.binding]
- [x] [serial] Reproduce colliding-site guidance loss and route stable JSON guidance through accepted full identity. r[chaoscontrol.stable_json_guidance.binding]
- [x] [serial] Check colliding-site payload/identity attribution, unresolved-call rejection, feature configurations and actual local output; update parity documentation and record evidence. r[chaoscontrol.stable_json_guidance.rejection]

## Evidence (2026-10-28)

- Source baseline: `411a9a0b` plus this change. Owner: ChaosControl SDK maintainers. Consumer: Rust workloads using stable JSON exploration guidance. Existing macro callers adopt the fix without changes; direct callers can use `emit_json_with_key` for full-key binding or `emit_json` for compatibility aliases.
- Baseline `cargo test -p chaoscontrol-sdk --test json_guidance`: 1 passed. Before the source correction, `cargo test -p chaoscontrol-sdk --test json_guidance_identity` failed with 0 observations versus 2 expected for accepted sites sharing alias `2418279297`, kind and message.
- After correction, `cargo test -p chaoscontrol-sdk --test json_guidance --test json_guidance_identity`: 2 passed. The completed SDK suites also cover the expanded single-site regression rejecting an undeclared colliding key, alongside missing namespace/key, wrong kind/message, distinct fingerprint attribution, accepted tokens, payloads, direction and caller location.
- `cargo test -p chaoscontrol-sdk`: 113 passed, 21 ignored. `cargo test -p chaoscontrol-sdk --all-features`: 116 passed, 21 ignored. `cargo test -p chaoscontrol-sdk --no-default-features --test bundle_assertions_no_std`: 8 passed. `cargo check -p chaoscontrol-sdk --no-default-features --lib` passed with two existing dead-code warnings in `coverage.rs`.
- Actual `cargo run -q -p chaoscontrol-sdk --example stable_guidance_smoke` printed accepted left/right observations with distinct fingerprints and retained payloads, and confirmed no observation for a missing namespace/key. Its example and local JSONL were removed after execution.
- `cargo check --workspace --all-targets`, `cargo clippy -p chaoscontrol-sdk --all-targets --all-features -- -D warnings`, and `cargo fmt --all -- --check`: passed.
- Regenerated the Nickel product-scope projection. `cargo run -q -p chaoscontrol-evidence --bin check-product-scope -- --root . --write` and `cargo run -q -p chaoscontrol-evidence --bin check-evidence-contracts -- --root .`: passed.
- Cargo commands used `TMPDIR=/home/brittonr/.cargo-target/tmp`. kache reported cache-storage I/O errors and compiled uncached; compilation and checks succeeded.
- Immediate outcome: colliding stable sites retain separately bound observations, and undeclared aliases cannot impersonate an admitted site. Durable capability: exact stable-key JSON guidance with behavioral regressions. This evidence proves local SDK instrumentation only, not guidance effectiveness, VMM execution, replay or release readiness.
- Final proposal/design/tasks gates passed and native sync installed both implemented requirements; this change remains active rather than archived. Traceability recognizes 400 requirements with none missing. Its overall verdict is still failed on the same nine unrelated native-process-profile and SMR-storage-recovery dangling IDs; neither new guidance ID is dangling. Global validation was not repeated to reconfirm the previously recorded unrelated native-process task-line blocker.
