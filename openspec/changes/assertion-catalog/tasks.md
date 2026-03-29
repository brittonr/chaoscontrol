## 1. Catalog Infrastructure in SDK

- [ ] 1.1 Add `linkme = "0.3"` to chaoscontrol-sdk Cargo.toml (supports no_std)
- [ ] 1.2 Create `src/catalog.rs` module with `CatalogEntry { id: u32, message: &'static str, kind: AssertKind, file: &'static str, line: u32 }` and `AssertKind` enum
- [ ] 1.3 Define `#[distributed_slice] static ASSERTION_CATALOG: [CatalogEntry]`
- [ ] 1.4 Add `catalog_entries() -> &[CatalogEntry]` accessor and `catalog_count() -> usize`
- [ ] 1.5 Implement compact serialization for catalog entries (length-prefixed, no serde needed)

## 2. Protocol Extension

- [ ] 2.1 Add `CMD_SEND_CATALOG = 0x0A` to chaoscontrol-protocol command IDs
- [ ] 2.2 Define catalog payload format: entry_count (u32) followed by packed entries (id, kind, message_len, message_bytes, file_len, file_bytes, line)
- [ ] 2.3 Implement encode/decode functions in protocol crate

## 3. Macro Changes

- [ ] 3.1 Modify `cc_assert_always!` macro to emit `#[distributed_slice(ASSERTION_CATALOG)] static _ENTRY: CatalogEntry = ...` alongside the function call
- [ ] 3.2 Apply same pattern to `cc_assert_sometimes!`, `cc_assert_reachable!`, `cc_assert_unreachable!`, `cc_assert_always_or_unreachable!`
- [ ] 3.3 Keep bare function API (`assert::always()` etc.) unchanged for callers not using macros

## 4. VMM and Oracle Integration

- [ ] 4.1 Handle CMD_SEND_CATALOG in `handle_sdk_hypercall()` — deserialize and pass to FaultEngine
- [ ] 4.2 Add `pre_populate_from_catalog(entries: &[CatalogEntry])` to PropertyOracle — creates AssertionRecord with status=Unexercised for each entry
- [ ] 4.3 Update `OracleReport` to include unexercised count and list of unexercised assertion IDs
- [ ] 4.4 Update `format_report()` to print unexercised assertions section

## 5. Guest Integration

- [ ] 5.1 Send catalog in `lifecycle::setup_complete()` — serialize ASSERTION_CATALOG slice, send via CMD_SEND_CATALOG hypercall before the setup_complete hypercall
- [ ] 5.2 Graceful fallback: if hypercall fails (old VMM), log warning and continue

## 6. Testing

- [ ] 6.1 Unit test: macro registers catalog entry with correct id, message, kind, file, line
- [ ] 6.2 Unit test: catalog serialization round-trips correctly
- [ ] 6.3 Unit test: oracle pre-populated entries show as Unexercised in report
- [ ] 6.4 Unit test: after assertion fires, pre-populated entry transitions from Unexercised to Exercised
- [ ] 6.5 Unit test: guest without catalog still works (oracle report has zero unexercised)
- [ ] 6.6 Run `cargo clippy --workspace` clean
