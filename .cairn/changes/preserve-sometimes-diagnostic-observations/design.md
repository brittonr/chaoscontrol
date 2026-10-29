## Decision

Keep the existing counter and API; define `LocalDryRunReport.failed_assertions` explicitly as observed safety-violation hits, not distinct failed properties. Continue incrementing each site's raw `failure_count` for a false sometimes observation, but do not increment the report-level observed-violation count for that kind. Never-successful sometimes sites remain listed in `sometimes_without_success` and `gaps()`. Always/reachability observations keep their current counter behavior, including repeated violations.

## Scope and compatibility

Language-server references show the counter is assigned only in the diagnostic parser. No strict-evidence dependency, shared policy extraction or duplicate verdict engine is needed: this parser does not evaluate strict property verdicts and remains diagnostic-only. Existing per-site raw success/failure totals are unchanged. Do not alter legacy metadata, optional-property/reachability handling, strict reader decisions or the separate campaign-wide liveness change.

## Verification

Baseline the existing workload tests. Reproduce the false-positive count using a real SDK-instrumented external consumer with false/true/false progress attempts. Strengthen the existing eventual-progress test and add paired no-success/safety cases; run them failing before and passing after the fix. Run actual eventual-success, stalled-progress and safety-failure processes through both the SDK diagnostic API and strict report/quality CLIs. The positive gate must pass; stalled progress and safety negatives must still fail the strict gate for their actual reason. Run full/default/sparse SDK tests, no-full tests, workspace check, SDK Clippy, formatting, product-scope/evidence checks and scoped Cairn gates. Record known global gaps and remove temporary artifacts before publication.
