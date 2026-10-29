## Context

Canonical text is nonempty/control-free and bounded to MAX_EVENT_TEXT_BYTES before evaluation. Map projects JSON scalars from details; json_scalar converts strings/numbers/booleans into derived strings. Source digests already include original text, and derived fields are query-local. Query evaluation finishes before the CLI writes result/receipt artifacts.

## Decisions

Add a separate MapText query form, preserving Map's existing source and behavior. Reuse pointer/field admission, scalar conversion, derived-field/byte admission and canonical lookup precedence. Use the existing serde_json parser with its default recursion bound. Parse one admitted text at a time; discard the temporary parsed value after extracting one scalar. Do not clone the canonical text/details or retain a second parsed tree in each output event. Malformed JSON, trailing input and parser-depth excess fail the whole projection via a typed InvalidJsonText error, not partial rows or silent omission. Filter mixed plain-text logs to a JSON-producing source before MapText. Missing/non-scalar pointers in valid JSON do not insert/erase a derived value, matching Map.

Charge one existing logical operation per projected candidate row in addition to input work. Parsing is bounded per row by the existing 8192-byte text cap and parser depth, not an arbitrary-code operation count. Share query node/depth admission and moment propagation with Map. Preserve all canonical fields and receipt schema; query/pointer/field/source/result drift remains binding. No fallback parser, new limit fields or external preprocessing dependency.

## Verification

Capture baseline tests and failing-before serialized MapText query. Permanent regressions cover stdout projection/filtering, scalar types/escaped pointers, preservation of differing canonical details and identity, missing/non-scalar pointers, malformed/trailing/deep JSON, exact text-byte limits, field/pointer validation, operation/result/structural limits, inherited UpTo moments and query/source/result receipt drift. Exercise actual CLI query/validate success plus malformed/budget/drift failures and absence of partial artifacts. Run replay tests, Clippy, workspace compilation, formatting, contracts and scoped Cairn gates. Preserve/exclude the user's existing confined-pointer test.
