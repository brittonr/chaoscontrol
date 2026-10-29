## Context

The upstream map/add_fields example retains a utilization quotient. Derived values are strings; canonical kind/text/process/source_kind lookup wins over same-named projections. Existing FieldRatio already defines finite JSON-number operands, denominator-first division, zero/overflow nonmatches and binary64 rounding.

## Decision

Add MapRatio with input, output field, numerator and denominator names. Admit all three names with existing nonempty/control-free/256-byte UTF-8 rules before visiting rows, including empty inputs. Extract the existing quotient calculation into a private finite_field_ratio helper returning Option<serde_json::Number>, used by both FieldRatio and MapRatio; do not introduce a second numeric convention or broaden scalar coercion.

Evaluate the input and charge one operation per candidate row, just like Map. Admit each intermediate row before reading it and after writing it, as required to prevent an oversized input or generated quotient from being hidden by later Narrow or replacement. Compute the quotient before any write, so destination aliases of either operand are safe. For Some(number), reuse an existing destination String via clear/write!, allocating a key/value only for a new projection. For None, remove only the destination projection: unlike pointer extraction, recomputation must not leave an obsolete ratio that downstream numeric filters could mistake for current utilization. Canonical fields remain immutable, even when a destination has a canonical name; canonical lookup still wins.

No sorting/deduplication change: ordinary final canonical ordering and first-position projection precedence apply. Integrate admission and moment traversal; preserve child moments and reject mixed moments as before. Existing operation/node/depth/scan/field-count/derived-byte/result-prefix bounds apply. Receipts already serialize and bind the complete query and all canonical source/output projection facts, so no schema migration is needed.

## Verification

Capture baseline tests and exported EventQuery references, admit proposal/design/tasks, then reproduce missing MapRatio via a behavior test. Verify complete canonical rows, finite and undefined ratios, removal of stale output, both output/operand aliases, canonical precedence, signed zero/underflow/rounding, Map/MapText/count composition and downstream numeric filtering/narrowing. Verify three-name UTF-8/empty admission, shared work/structure/scan/result bounds, exact derived capacity/replacement and unhideable intermediate overflow, moment propagation/conflicts and expression/source/result receipt drift. Exercise actual CLI query/validate and rejected queries without partial artifacts. Run replay tests, Clippy, workspace compilation, formatting and scope/evidence checks. Synchronize the accepted requirement without archiving; preserve unrelated traceability failures and exclude the user's pointer test from publication.
