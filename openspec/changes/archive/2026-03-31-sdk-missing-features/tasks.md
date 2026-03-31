## 1. Guidance API (SDK → VMM)

- [x] 1.1 Create `crates/chaoscontrol-sdk/src/guidance.rs` with `guidance(message, distance)` and `guidance_with_id(id, distance)` functions (full mode: write f64 to result field, issue hypercall with `CMD_GUIDANCE`; no-op mode: empty stubs)
- [x] 1.2 Add `guidance` transport path in `transport.rs`: new `hypercall_guidance(id: u32, distance: f64)` that writes `distance.to_le_bytes()` into the result field at offset 0x10 and triggers with `CMD_GUIDANCE`, `payload_len = 0`
- [x] 1.3 Wire `guidance` module into `lib.rs` (pub mod, feature-gated) and re-export `guidance`, `guidance_with_id` from `prelude.rs`
- [x] 1.4 Add `CMD_GUIDANCE` arm to `FaultEngine::handle_hypercall` in `crates/chaoscontrol-fault/src/engine.rs`: read `page.id` and f64 from `page.result`, store in `guidance_values: HashMap<u32, f64>`, return `(0, STATUS_OK)`
- [x] 1.5 Add `guidance_values` field to `FaultEngine` struct, initialize as empty HashMap, expose with a `pub fn guidance_values(&self) -> &HashMap<u32, f64>` accessor
- [x] 1.6 Add tests: SDK unit test that `guidance` compiles in both full and no-op modes; fault engine test that stores and overwrites guidance values; test that NaN is stored without error

## 2. Comparison Macros with Auto-Captured Details

- [x] 2.1 Rewrite the 12 comparison macros (`cc_assert_always_lt!` through `cc_assert_sometimes_ne!`) in `assert.rs` to bind `$left` and `$right` to local variables, evaluate comparison once, and emit `{"left": format!("{:?}", left), "right": format!("{:?}", right)}` details on failure (empty `{}` on success)
- [x] 2.2 Add a second macro arm to each comparison macro accepting an explicit `$details` fourth argument that overrides auto-capture
- [x] 2.3 Update `cc_assert_always_some!` and `cc_assert_sometimes_some!` to auto-capture `{"value": "None"}` on failure
- [x] 2.4 Add tests: comparison macro with failing values verifies details contain left/right; passing values verify empty details; explicit details form overrides auto-capture; side-effecting operand evaluated once

## 3. Assert Implies Macro

- [x] 3.1 Add `cc_assert_implies!` macro in `assert.rs`: bind `$precondition` and `$conclusion` to locals, compute `!p || q`, auto-capture `{"precondition": format!("{:?}", p), "conclusion": format!("{:?}", q)}` on failure, register catalog entry with `CATALOG_KIND_ALWAYS`
- [x] 3.2 Add second arm accepting explicit `$details` fourth argument
- [x] 3.3 Re-export `cc_assert_implies` from `prelude.rs`
- [x] 3.4 Add tests: both-true passes, precondition-false passes (vacuous truth), implication violated captures details, explicit details form works, catalog registration with kind ALWAYS

## 4. Result Assertion Macros

- [x] 4.1 Add `cc_assert_always_ok!` macro: bind result to local, check `is_ok()`, on Err capture `{"error": format!("{:?}", result)}`, register catalog with `CATALOG_KIND_ALWAYS`
- [x] 4.2 Add `cc_assert_sometimes_ok!` macro: same pattern, register catalog with `CATALOG_KIND_SOMETIMES`
- [x] 4.3 Add `cc_assert_always_err!` macro: check `is_err()`, on Ok capture `{"ok_value": format!("{:?}", result)}`, register catalog with `CATALOG_KIND_ALWAYS`
- [x] 4.4 Add `cc_assert_sometimes_err!` macro: same pattern, register catalog with `CATALOG_KIND_SOMETIMES`
- [x] 4.5 Add explicit `$details` second arm to all four Result macros
- [x] 4.6 Re-export all four Result macros from `prelude.rs`
- [x] 4.7 Add tests: Ok/Err pass/fail for each macro, auto-captured error details on failure, explicit details override, side-effecting expression evaluated once, catalog registration with correct kind

## 5. Integration Verification

- [x] 5.1 Run `cargo check -p chaoscontrol-sdk` and `cargo check -p chaoscontrol-sdk --no-default-features` to verify both full and no-op modes compile
- [x] 5.2 Run `cargo test -p chaoscontrol-sdk` and `cargo test -p chaoscontrol-fault` to verify all new tests pass
- [x] 5.3 Run `cargo check -p chaoscontrol-guest -p chaoscontrol-raft-guest -p chaoscontrol-net-guest` to verify existing guests still compile with the new SDK
