## Tasks

- [x] [serial] Confirm that all assertion evidence today flows through the Rust SDK transport. r[chaoscontrol.fallback_assertion_transport.record_format]
- [ ] [depends:fallback-baseline] Define the versioned fallback record schema and keep it under the protocol crate. r[chaoscontrol.fallback_assertion_transport.record_format]
- [ ] [depends:fallback-schema] Add deterministic sink ingestion with replay-ordered validation. r[chaoscontrol.fallback_assertion_transport.deterministic_ingestion]
- [ ] [depends:fallback-ingestion] Bound the sink and emit typed overflow events. r[chaoscontrol.fallback_assertion_transport.bounded_sink]
- [ ] [depends:fallback-sink] Derive catalog identity for fallback records and fail on conflicts. r[chaoscontrol.fallback_assertion_transport.identity_conflict]
- [ ] [depends:fallback-identity] Extend bug reports and replay verdicts with process-scoped fallback evidence. r[chaoscontrol.fallback_assertion_transport.evidence_scope]
- [ ] [parallel] Add positive ingestion fixtures and negative malformed, conflict, overflow, reorder, and scope fixtures. r[chaoscontrol.fallback_assertion_transport.validation]
- [ ] [depends:fallback-validation] Run focused protocol, oracle, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.fallback_assertion_transport.validation]
