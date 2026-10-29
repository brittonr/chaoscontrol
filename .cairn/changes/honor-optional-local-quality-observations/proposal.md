## Why

Antithesis [always-or-unreachable](https://docs.rs/antithesis_sdk/0.3.0/antithesis_sdk/macro.assert_always_or_unreachable.html) passes when never encountered. ChaosControl's strict local verdict already recognizes `must_hit=false`, but its separate observation blocker ignores that metadata. A valid zero-hit optional always property therefore passes its verdict and still fails the pre-VM quality gate.

## What changes

Derive observation/progress blockers from the same existing property semantics used for verdicts, in both report production and report-total validation. Keep every catalog site, raw count, identity and observation flag. Required or legacy-unknown no-hit sites still block; observed violations still fail regardless of must-hit. Keep display-type/must-hit admission, schema and resource bounds unchanged.

## Capability and boundaries

Owner: ChaosControl evidence maintainers. Current consumer: Rust workloads and third-party assertion frameworks using optional always properties before VM promotion. Immediate outcome: a valid quiet optional property no longer creates a false pre-VM blocker. Durable contribution: consistent report/validator decisions with positive/negative runtime regressions and an SDK-to-quality procedure. This is local instrumentation parity, not VM execution, replay, hosted-platform equivalence or release readiness.
