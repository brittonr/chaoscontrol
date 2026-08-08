# Separate replay probes from assertion promotion gaps

## Why

Accepted workload proofs intentionally use snapshot replay probes that fail only after restored parent context. The assertion-readiness report currently counts those probe assertions as ordinary non-passing instrumentation gaps, leaving every accepted workload with a promotion blocker even though the probe is the replay-proof signal itself.

## What Changes

- Separate replay-probe failures from ordinary non-passing assertion gaps in the generated assertion-readiness surface.
- Preserve replay-probe visibility as checked replay-proof signal evidence, not as an instrumentation-readiness promotion blocker.
- Keep fail-closed checks for hidden replay-probe signal counts and for ordinary unhit/uncategorized/non-passing gaps.

## Impact

- **Specs**: assertion-catalog promotion semantics distinguish replay-probe failures from instrumentation gaps.
- **Code**: `chaoscontrol-evidence` report rendering, parsing, checker selftests, and model tests.
- **Docs**: regenerated `docs/assertion-readiness-status.md`.
