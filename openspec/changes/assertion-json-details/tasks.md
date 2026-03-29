## 1. Details Helper Module

- [ ] 1.1 Create `crates/chaoscontrol-sdk/src/details.rs` module, re-export from `assert::details`
- [ ] 1.2 Add `keys` submodule with constants: NODE_ID, TERM, ROLE, LOG_INDEX, LOG_LENGTH, COMMIT_INDEX, PEER_ID, MESSAGE_TYPE, FROM, TO, DELIVERED, FAULT_TYPE, TARGET
- [ ] 1.3 Implement `node(id: usize, term: u64, role: &str) -> serde_json::Value`
- [ ] 1.4 Implement `log(index: usize, term: u64, length: usize) -> serde_json::Value`
- [ ] 1.5 Implement `network(from: usize, to: usize, delivered: bool) -> serde_json::Value`
- [ ] 1.6 Implement `fault(fault_type: &str, target: usize) -> serde_json::Value`
- [ ] 1.7 Implement `merge(a: &Value, b: &Value) -> Value` for combining detail objects
- [ ] 1.8 Unit tests for all helpers (correct keys, correct values, merge combines both)

## 2. Oracle Detail Storage

- [ ] 2.1 Add `last_details: Option<Vec<u8>>` to `AssertionRecord` in PropertyOracle
- [ ] 2.2 Store details bytes on assertion failure (not on every pass — too expensive)
- [ ] 2.3 Include `last_failure_details` in OracleReport per failed assertion
- [ ] 2.4 Unit test: failed assertion stores details, passing assertion does not overwrite

## 3. Report Display

- [ ] 3.1 Update `format_report()` to print details for failed assertions as indented JSON
- [ ] 3.2 Update TriageReport to include assertion details in failure summary
- [ ] 3.3 Unit test: format_report output includes details for a failed always assertion

## 4. Call Site Migration

- [ ] 4.1 Update chaoscontrol-raft-guest/src/main.rs — safety assertions use `details::node()` + `details::log()`
- [ ] 4.2 Update chaoscontrol-raft-guest/src/main.rs — liveness assertions use `details::node()`
- [ ] 4.3 Update chaoscontrol-guest/src/main.rs — replace ad-hoc json with appropriate helpers
- [ ] 4.4 Update chaoscontrol-net-guest/src/main.rs — use `details::network()` for networking assertions
- [ ] 4.5 Verify assertion IDs unchanged (message strings must be identical)

## 5. Verification

- [ ] 5.1 Run `cargo test -p chaoscontrol-sdk` — new module tests pass
- [ ] 5.2 Run `cargo test -p chaoscontrol-fault` — oracle detail storage tests pass
- [ ] 5.3 Run `cargo test --workspace` — all existing tests still pass
- [ ] 5.4 Run `cargo clippy --workspace` clean
