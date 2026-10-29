## Why

Antithesis `assert::assert_raw` preserves the caller's string ID in fallback declarations and hits (see upstream 0.3.0 `assert_raw` documentation and its `42-1005` example). ChaosControl accepts that ID as a raw stable logical key, but emits its compact alias formatted as hexadecimal in local `antithesis_assert.id`. Third-party adapters cannot correlate the resulting fallback records by their supplied IDs. The evidence reader also requires the old hexadecimal format, so both producer and consumer must change together.

## Change

Preserve the stable logical key of the `raw` namespace in local declaration and hit IDs. Carry the borrowed ID alongside descriptor-resolved source metadata, without duplicating registration storage or rescanning raw registrations. Keep runtime and compiled raw-namespace registration consistent, since registration provenance is not encoded in the descriptor. Update the evidence reader's catalog projection, strict event admission and stored-report verification to the same canonical rule. Separate local report formatting from the numeric VM compatibility alias so VM emission no longer constructs a discarded local string. Retain ordinary compiled-site ID formatting outside the raw namespace, fingerprints, catalog tokens, registration admission and late-site rejection.

## Scope

Local Rust SDK fallback instrumentation and its evidence reader only. No VM protocol, replay, host-service or release-readiness claim. Owner: ChaosControl SDK/evidence maintainers; consumer: third-party Rust assertion adapters and local fallback tooling. Existing adapter callers adopt the fix unchanged. The corrected raw-ID representation is a clean cutover: older raw JSONL and derived reports using compact aliases need regeneration; ordinary compiled streams remain unchanged.
