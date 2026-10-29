## Decision

Reuse `emit_registered_assertion<S: ?Sized>`'s no-op signature convention for `emit_registered_always_or_unreachable`. Keep the latter's empty body and all macro callsites unchanged. Do not require `Serialize`, `Clone`, `Sized`, `std` or new dependencies when instrumentation is disabled. The full-mode serialization boundary and must-hit/display semantics are unchanged.

## Verification

Run the existing no-default-features behavioral suite first. Add a regression using a non-Serialize detail struct and side-effecting condition/detail expressions for both boolean outcomes; check exactly-once condition-before-details evaluation. Cover unsized borrowed string/slice details. Observe its compile failure before the fix and passing runtime assertions afterward.

Compile/run a temporary downstream consumer with identical serializable callsites under full and no-full configurations; also compile its no-std library surface. Verify full-mode output and disabled-mode lack of output using the actual program. Preserve optional-property metadata and const-message rejection. Run focused/default/sparse SDK tests, workspace/feature checks, Clippy, formatting and evidence/product-scope checks, then sync the implemented native requirement. Remove smoke scaffolds and preserve unrelated lifecycle failures and user changes.
