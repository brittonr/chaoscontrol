## Context

Antithesis map transforms events with arbitrary expressions; the existing bounded local MapRatio supplies numeric projection, stale-value clearing, alias safety, capacity admission and receipt patterns. finite_field_number already implements complete finite JSON-number text with canonical-name precedence.

## Decision

Add MapDifference with input, destination field, minuend and subtrahend selectors. Admit all three names even on empty input. Evaluate the child, charge one operation per materialized child row, admit each input row before projection. Resolve the minuend first and then subtrahend using finite_field_number, before any destination mutation. Subtract using binary64; convert through serde_json::Number::from_f64. Missing/malformed/nonfinite operands or an overflowing subtraction produce no value: retain the row but remove stale destination output, matching MapRatio. Valid signed zeros, subnormals, wide-number rounding and floating-point cancellation follow ordinary binary64, not exact arithmetic.

For finite results reuse the existing destination String or insert a new projection; preserve all canonical evidence and other fields. Destination aliases of either operand are safe. Canonical names still win over derived shadows, including an absent canonical process. Admit generated rows immediately, so Narrow/Filter/final deduplication cannot hide overflow. Integrate child moment traversal and existing shared node/depth/scan/work/derived/result/prefix and receipt rules. Receipts bind operand order, all three names, input, canonical source facts and output; old query/schema forms remain unchanged and new queries need upgraded readers. No new error, compatibility shim or generalized arithmetic abstraction.

## Verification

Capture replay baseline and exported query references; admit gates before implementation and reproduce unsupported MapDifference. Regress full rows for signed/zero/fractional/rounding/underflow/overflow/invalid/missing operands, aliases and canonical precedence, JSON projections, RunningCount and past-witness deltas, downstream numeric selection/narrowing, all names, intermediate/generated capacity, shared bounds, moments and expression/source/result receipt drift. Exercise actual CLI success/undefined/failure paths without partial evaluation artifacts. Run replay tests, Clippy, workspace compilation, formatting and scope/evidence checks; synchronize accepted requirement without archiving. Stage only owned source hunks.
