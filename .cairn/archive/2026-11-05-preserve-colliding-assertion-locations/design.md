## Decision

Use the accepted fingerprint to select a descriptor for hit metadata, and reuse the descriptor already available while emitting a declaration. Share a descriptor-based metadata matcher. Compare borrowed fields rather than reconstructing descriptors, hashing candidates, formatting source keys or allocating a location cache. Automatic logical keys are derived from the matched file/line/column; stable keys compare their exact key strings.

Keep ambiguity handling: multiple matching compiled entries still return no location, and raw registrations match the entire descriptor. Preserve raw display type and must-hit overrides. Do not loosen the compatibility assertion resolver: an ambiguous unqualified alias remains rejected. Only already-bound stable hits and their declarations gain correct location attribution.

## Verification

Use the fixed cross-namespace FNV collision with identical kind/message at distinct Rust functions. Match records by their accepted fingerprint and verify each declaration and hit carries the correct function/class/file and the descriptor's line/column. Keep two distinct fingerprints, unchanged details and aggregate verdict fields. Probe an undeclared stable call and an ambiguous unqualified alias; neither may create an accepted hit.

Run existing stable, fallback and raw regressions, SDK feature suites, no-full shape tests, workspace check, SDK Clippy, formatting, product-scope/evidence contract checks, and an actual local-output example. No KVM/protocol semantics change; evidence remains local. Keep the native change active and retain unrelated lifecycle failures.
