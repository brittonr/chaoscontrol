## Why

The net accepted-verdict rail drifted because wrapper defaults, curated proof evidence, and operator claims were not bound to one expectation source. A stale default could run successfully as a static gate while the live selected dogfood path failed to find the targeted accepted verdict.

## What Changes

- Add a committed dogfood expectation lockfile that records the expected accepted verdict class and default proof probe parameters per workload.
- Make Nix-generated accepted-verdict wrappers derive fail-after/max-attempt defaults from that lockfile.
- Extend static readiness validation and replay-readiness receipts so wrapper defaults, expected verdicts, and observed dogfood summaries are checked together.

## Impact

- **Files**: `dogfood-results/accepted-dogfood-expectations.json`, `flake.nix`, readiness validation scripts/docs.
- **APIs**: Existing commands stay stable; receipt JSON gains dogfood expectation fields.
- **Testing**: Static config validation, replay-readiness check, and targeted receipt-summary tests.
