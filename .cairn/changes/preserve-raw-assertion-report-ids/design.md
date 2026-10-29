## Decision

Resolve hit metadata through its accepted fingerprint and declarations through their descriptor, as before. Return source metadata plus an optional borrowed raw stable key from that same lookup. Stable keys in the `raw` namespace have raw report-ID semantics whether registered at runtime or compiled; other compiled entries have no raw report ID. Keep stored raw source metadata unchanged. A small borrowed metadata result avoids additional allocation, registration copies, locks or scans.

Use a local-only report-ID formatter: borrow an admitted raw-namespace ID exactly, otherwise retain the eight-digit compact alias or full fingerprint fallback. VM wire calls continue using `compatibility_id.unwrap_or(0)` directly. Remove the obsolete combined wire/report-ID helper and migrate its callers. No runtime configuration changes.

The evidence reader derives report IDs from the accepted descriptor for catalog projection, event admission and report verification. Update its shared formatter to return the raw namespace's stable key, borrowing it during validation and owning it only when constructing a stored report entry. Apply this rule to both runtime and compiled raw-namespace declarations: registration provenance is not encoded in the descriptor, so producer and reader must not use incompatible rules. Reject inconsistent IDs; do not retain the old hashed raw-ID format as an alternate dialect. Regenerate historical local raw streams/reports rather than expecting mixed-version compatibility. Fingerprints and catalog tokens are unchanged.

## Verification

Extend the existing isolated raw integration test to check declarations and hits retain a named ID and an ID containing slash, quotes and Unicode through JSON round-trip, and that both records retain the same accepted fingerprint. Cover compiled raw-namespace registration as well as runtime registration. Retain late-site rejection and source/display/must-hit assertions. Verify compiled report-ID and fingerprint fallbacks, colliding-site source attribution and existing fallback schema tests. Add a consumer regression for unobserved catalog IDs, successful strict reports, wrong or substituted IDs, and forged stored report IDs.

Run SDK default/all-feature and no-full checks, evidence tests, workspace check, denied-warning SDK/evidence Clippy, formatting and product-scope/evidence contract checks. Exercise actual fallback output and its strict evidence consumer with a temporary example and remove it afterward. Record only local instrumentation evidence, sync scoped requirements and keep this change active; do not repair or hide unrelated lifecycle failures.

## Declaration admission found by actual smoke

The first actual producer-to-reader CLI run failed at a valid `hit = false` declaration. The reader previously admitted only hit records, so updating ID formatting alone was not an end-to-end fix. Resolve record kind from its assertion/display labels, check the hit's reachability condition only for hits, and require false conditions for declarations. Then apply the same catalog binding, redundant metadata, exact-ID and details checks to both record forms. The existing summarizer records declaration metadata but skips observation counters for `hit = false`. Cover all four assertion kinds, malformed declarations, unchanged zero-hit verdicts and the real SDK-to-CLI stream.

