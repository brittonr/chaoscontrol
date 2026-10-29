## Decision

Change only the full `assert_raw_with_key` parameter from `&Value` to `&S`, with `S: serde::Serialize + ?Sized`, matching `emit_registered_assertion`. Reuse `to_json_bytes` directly. Identity resolution must still precede sampling, which must still precede serialization. No eager `to_value`, cloned payload, wrapper or alternate API. Existing no-full generic borrowing remains unchanged.

## Consumers

The four stable macros all reach this emitter through `__cc_assert_stable`. Language-server references also identify the location-identity and sampling-identity integration tests; their existing `Value` arguments remain valid. No caller migration or re-export change is needed.

## Verification

Run the existing stable/sampling tests before editing. Reproduce E0308 in an external borrowed-struct consumer. Add a behavioral regression that checks typed details and outcomes for all stable assertion kinds, unsized string/slice payloads, serializer call counts under sparse sampling and unknown-site rejection before serialization. Run default/sparse variants plus full SDK tests and existing no-full tests. Run an actual external no-std library consumer in full/no-full modes with identical stable calls; inspect local JSONL and strict report admission for the full path, and assert no output without full. Run workspace compilation, SDK Clippy, formatting, product-scope/evidence checks and scoped Cairn gates. Preserve user changes and unrelated global lifecycle failures; retain only summarized local instrumentation evidence.
