## Why

Molten needs to hand semantic operation histories to ChaosControl and receive checker reports it can verify independently. The existing checker API validates report shape, but a structurally plausible report with a forged witness or a fabricated `valid` verdict could pass shape validation. There is also no provider entry point that imports exact schema versions, persists readback, and exposes synthetic fixtures without implying guest execution.

## What Changes

- Add a pure provider contract module (`semantic_history::contract`) with exact v1/v2 import dispatch, original-schema export, checking, proof validation, synthetic fixtures, and a mutation self-test.
- Make `validate_semantic_report` recompute the deterministic bounded checker result and compare verdict, witness, invalid/unknown proof, and reduction. Allow an empty witness for an admitted empty history.
- Require exact schema version 1 for legacy evidence values, strictly increasing reference adapter indices, and completion metadata that matches its earlier invocation.
- Add the `semantic-history-contract` CLI (`import`, `check`, `validate`, `fixtures`, `selftest`) with bounded regular-file inputs, and a packaged register history example.

## Impact

- **Files**: `crates/chaoscontrol-evidence/src/semantic-contract.rs`, `semantic_history.rs`, `src/bin/semantic-history-contract.rs`, `fixtures/semantic-history-register.json`, `docs/semantic-history-v2.md`, `README.md`.
- **Testing**: Evidence crate tests, the CLI self-test, fixture generation with check/validate readback, and forged-report negative controls.
- **Claims**: Reports remain bounded checker results over admitted finite histories. Synthetic fixtures are not guest observations, execution receipts, or reference-tool execution evidence.
