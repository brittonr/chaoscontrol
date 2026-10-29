## Decision

Remove the unconditional object gate at the SDK bound transport boundary. Parse valid JSON and preserve its shape; require an object only when `CHAOSCONTROL_PROCESS_ID` requires process metadata insertion. Validate the process token and reject non-object scoped details as before. Reuse the current event frame, protocol byte bound and local record emitter; do not wrap values, add fields to the wire schema or fabricate metadata.

In strict local evidence, allow non-object details to carry no guest/category overrides. Object overrides still require strings matching the accepted descriptor. Legacy records default absent metadata to uncategorized and remain legacy-ambiguous: admitting their JSON shape must not make them promotable. Omitted details, unknown fields, identity/display/must-hit conflicts and JSON/stream limits remain errors. SDK diagnostic behavior already permits JSON details and needs no redundant implementation.

## Verification

First reproduce both silent SDK loss and strict-reader rejection. Test all JSON shapes, unchanged outcomes/identities, no metadata inferred from scalar text or nested arrays, and legacy non-promotion. Negative tests retain omitted-details rejection, wrong object metadata, mismatched identities and excessive JSON. SDK tests isolate initialization and sparse sampling in child processes and preserve scoped object metadata plus scoped non-object rejection.

Run an actual temporary Rust SDK example through diagnostic and strict report/quality paths, including a failing assertion and malformed/oversize negatives; remove the temporary code/output afterward. Run focused/full SDK/evidence suites, sparse-feature tests, workspace check, denied-warning Clippy, formatting and Nickel/evidence/product-scope checks. Sync tested native Cairn requirements; preserve unrelated lifecycle failures and user changes.
