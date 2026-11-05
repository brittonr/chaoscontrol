## Decision

Pass admitted `must_hit` metadata into the existing `blocks_as_unobserved` helper and use `derive_local_verdict` for its zero-hit decision. Reuse that same verdict to classify unmet sometimes/reachable progress in report generation and total recomputation. Aggregate the report's existing counters and blocker lists in one catalog pass; remove the obsolete success-ID sets rather than retaining duplicate progress state. Do not allocate new per-site state or change public schemas.

## Invariants

An optional unobserved site remains registered with `observed=false` and zero counts, but a passing verdict creates no observation/progress blocker. Required and missing-metadata cases remain conservative. Observed false always/sometimes conditions and reached unreachable sites retain their failed verdicts. A caller cannot suppress required gaps or alter aggregate counts without report validation rejecting the discrepancy. Existing strict display/must-hit pairing and identity/counter checks remain authoritative.

## Verification

Baseline strict identity tests, reproduce the public raw optional declaration rejected by the real report/quality CLIs, and keep a failing-before/passing-after regression. Cover optional quiet, required quiet, missing metadata, observed optional violation, required progress gaps and invalid/tampered reports. Run actual SDK-only consumer processes for quiet optional, quiet required and optional violation; inspect identity/counters and quality results. Run evidence/SDK tests, workspace compilation, evidence Clippy, formatting, scope/evidence checks and native Cairn gates. Record only local instrumentation evidence, preserve unrelated user/global lifecycle work, and remove temporary smoke artifacts before publication.
