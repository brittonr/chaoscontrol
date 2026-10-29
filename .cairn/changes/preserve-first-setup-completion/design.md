## Decision

Keep validation before idempotence. In the strict evidence reducer, deserialize the exact setup envelope, check status and object metadata limits, then ignore an already-complete transition. In the SDK diagnostic reducer, validate the exact record before ignoring repeats. It must admit an explicit `details` field of any JSON type, matching the SDK and canonical reader; extract metadata only from a top-level object. Preserve each reader's existing generic input preflight and resource bounds and strict evidence's additional object metadata policy.

The first valid setup increments the semantic `setup_complete` count once and supplies setup adoption metadata. Later valid setup records do neither, even when their details differ. Continue processing following assertion/event records. Do not add SDK-global atomic state: emitting each SDK call matches upstream and permits the reader to observe and validate the input stream. No raw records are discarded before validation and no schema/version changes are needed.

Update the canonical setup-details bounds requirement via a modified native delta: repeat validation remains required, but repeated valid setup is no longer an error. Older change receipts/spec deltas remain historical evidence for their recorded revisions, not an instruction to restore the old rejection.

## Verification

Reproduce both readers' repeated-setup failures. Test identical and differing repeats, first object metadata versus first null/scalar/array, no track replacement/inflation, one semantic transition and unchanged assertion counts. Include an event after repeats to ensure reduction continues. Negative cases put malformed status, missing details, extra fields, excessive metadata (strict reader), deep JSON and excess string data after a valid setup; these must still fail.

Run focused and full SDK/evidence tests, feature checks, workspace check, denied-warning Clippy, formatting and evidence/product-scope checks. Exercise an actual SDK program emitting several setup calls through both its diagnostic reader and the strict report/quality CLIs. Remove temporary smoke code/artifacts after proof, sync implemented requirements and retain unrelated global lifecycle failures. Evidence remains local instrumentation only.
