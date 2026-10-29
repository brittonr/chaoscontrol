## Decision

Deserialize setup details as `serde_json::Value`, not `Map<String, Value>`. Borrow an object view when present to apply the existing metadata field/key bounds and top-level adoption-track extraction. Do not wrap scalar values in synthetic objects, discard the value, relax surrounding record admission, or add a second parser. Keep generic preflight/byte/line/event limits on every input shape.

The `details` field remains required: explicit JSON null is a valid value, omission is not. Preserve strict unknown-field/status and duplicate-setup rejection. Array members and scalar text are workload data, not evidence metadata; nested `adoption_track` text must not promote tracks. No SDK signature, VM wire, report schema or identity changes.

## Verification

Reproduce null rejection with a strict catalog/hit fixture, then cover null, both booleans, integer/floating values, quoted Unicode strings, empty/nested arrays and objects through report generation and quality validation. Retain object metadata limits at their documented boundaries and malformed/duplicate/missing record rejection. Verify non-object details still obey generic JSON nesting/byte bounds.

Exercise actual SDK programs with representative details through the report CLI and assertion-quality CLI; remove the temporary example and artifacts after proof. Run focused and full evidence tests, SDK regressions, workspace check, denied-warning evidence Clippy, formatting, product-scope/evidence contract checks and scoped Cairn gates. Sync implemented requirements but keep this native change active and preserve unrelated lifecycle failures.
