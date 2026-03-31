## 1. SDK record_state (chaoscontrol-sdk)

- [ ] 1.1 Add `coverage::record_state(pairs: &[(&str, &str)])` — FNV-1a hash each pair into 2 code-region bitmap slots
- [ ] 1.2 No-op when coverage not initialized (same guard as `record_edge`)
- [ ] 1.3 Add `#[cfg(not(feature = "full"))]` no-op stub
- [ ] 1.4 Unit tests: different values → different slots, key domain separation, no-op without init

## 2. Explorer event enrichment (chaoscontrol-explore)

- [ ] 2.1 Add `enrich_with_protocol_events(coverage: &mut CoverageBitmap, report: &OracleReport)` — hashes event names + detail KVs into assertion region
- [ ] 2.2 Hash scheme: `hash("event:" ++ name)` for event name, `hash("event:" ++ name ++ ":" ++ key ++ "=" ++ value)` for each top-level detail
- [ ] 2.3 Wire into all 3 enrichment call sites (fault-schedule, input-tree, probe) alongside existing enrichment calls
- [ ] 2.4 Unit tests: different events → different coverage, no events → no enrichment, event name hashed

## 3. Explorer assertion-detail enrichment (chaoscontrol-explore)

- [ ] 3.1 Extend `enrich_with_assertion_state` to hash top-level JSON detail keys from assertion records
- [ ] 3.2 Hash scheme: `hash("assert:" ++ message ++ ":" ++ key ++ "=" ++ value)` for each top-level detail key
- [ ] 3.3 Limit to top-level keys only — nested values stringified
- [ ] 3.4 Unit tests: different detail values → different coverage, null details → no extra hashing

## 4. Raft guest migration (chaoscontrol-raft-guest)

- [ ] 4.1 Replace `record_edge(10000 + leader_count.min(3))` and similar protocol-state edges with `record_state` calls
- [ ] 4.2 Keep structural edges (tick × active node, alive_mask × partition_count) as `record_edge`
- [ ] 4.3 Rebuild initrd after migration (`nix build .#initrd-raft`)
- [ ] 4.4 Verify exploration still finds bugs with migrated guest (spot check: skip_truncate variant)

## 5. Cleanup and validation

- [ ] 5.1 Run `cargo test` — all tests pass
- [ ] 5.2 Run `cargo clippy --all-targets -- -D warnings` — clean
- [ ] 5.3 Run `cargo fmt --all`
