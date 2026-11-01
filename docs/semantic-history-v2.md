# Semantic history v2

Semantic history v2 records client-visible operation events. It does not replace a VMM replay trace.

## Event rules

Each operation starts with one `invoke` event. An `ok`, `fail`, or `info` event can complete the operation.

- `ok` means that the operation took effect.
- `fail` means that the operation did not take effect.
- `info` means that the operation can have taken effect.
- A pending invocation has no completion event.

A retry keeps the logical operation ID. It uses a new attempt ID and names the prior attempt.

The admission core rejects duplicate attempts, orphan completions, changed retry content, noncanonical event order, and incorrect completeness counts.

## Identity

The v2 identity uses domain-separated BLAKE3. It binds the profile, model, bounds, completeness counts, event order, and event content.

JSON is only a transport format. JSON object-field order does not change the semantic identity.

History v1 keeps its SHA-256 transport digest. A v1 result is completion-order evidence. It is not v2 linearizability evidence.

The provider import dispatcher accepts only exact schema versions 1 and 2.
Version 1 is decoded as the existing typed `OperationHistory`; its digest remains
the existing SHA-256 of that typed JSON serialization, including its original
field order. The legacy evidence-value helper retains its separate SHA-256
value-serialization behavior. Neither identity is silently converted to v2.
Version 2 is fully admitted before import succeeds. Same-schema export/readback
preserves the admitted data and identity; JSON whitespace is not an identity.

## Checker verdicts

The checker derives real-time precedence from invocation and completion intervals. It evaluates pure register and compare-and-swap models.

The checker returns one of these verdicts:

- `valid`: A retained witness gives one legal linearization.
- `invalid`: No legal model transition exists in the admitted finite search.
- `unknown`: Pending evidence, an unsupported model, or a declared bound stopped evaluation.

A bound failure cannot produce `valid`.

Report validation recomputes the deterministic bounded checker result, comparing
the verdict, complete witness, invalid/unknown proof and reduction. A structurally
plausible report is insufficient: changed source identity, forged state hashes,
unsupported-model `valid` claims and mismatched proofs fail validation. Empty
admitted histories may have a valid empty witness. Reference status is retained
metadata, not independently verified reference-tool execution evidence.

The profile limits operation count, states, branches, depth, memo bytes, and reduction attempts. Independent-key decomposition needs an explicit key-isolation property from the model.

## Invalid witnesses

An invalid report retains the failed operation set and model-state identity. The reducer removes paired operations only. It reports `locally_reduced` or `budget_limited`.

A `budget_limited` result is not minimal.

## Reference checker boundary

The Jepsen-compatible adapter imports and exports typed operation events. The external reference checker is not part of the pure core.

A conformance report binds both tool identities and both verdicts. A disagreement blocks promotion. It does not select one checker as authoritative.

The external tool is optional outside the conformance rail. It is not a runtime dependency for ChaosControl.

Adapter indices must strictly increase; completion process, key and function must
match the earlier invocation. Import does not discard mismatched completion
metadata. The adapter reconstructs logical IDs and ordinal time; it is not a
lossless transport for original timestamps, retries or environment events.

## Timeline rules

Text, JSON, and static HTML views use one semantic timeline projection. The projection includes operation intervals, latency, lifecycle phases, witness membership, and fault events.

A selected fault is not an applied or observed fault. The timeline shows these phases separately. Temporal overlap does not prove causation.

## Claim limits

A `valid` verdict applies only to the admitted finite history, model, and bounds. It does not prove:

- system correctness;
- checker soundness;
- exhaustive schedule coverage;
- deterministic replay;
- fault effect;
- durability or transaction semantics;
- security; or
- release readiness.

Downstream adapters must retain the history ID, profile ID, model, bounds, completeness counts, verdict, witness, reference status, scope, and non-claims.

## Provider CLI and synthetic fixtures

The Cargo binary `semantic-history-contract` exposes:

```text
semantic-history-contract import INPUT OUTPUT
semantic-history-contract check HISTORY REPORT
semantic-history-contract validate HISTORY REPORT
semantic-history-contract fixtures DIRECTORY
semantic-history-contract selftest
```

`import` persists the original schema and verifies readback. `check` emits and
readback-validates the report. Successful execution means the input was admitted
and checked, **not** that its verdict is `valid`: consumers must inspect the
report's exact verdict. `validate` rejects stale sources or mutated proofs.
Inputs are regular files bounded to 16 MiB. Outputs require an existing parent
directory; `fixtures` creates its selected directory.

`fixtures` writes `valid`, `invalid`, `unknown-model`, `unknown-pending`, and
`unknown-bound` history/report JSON pairs, plus `legacy-history.json`,
`legacy-report.json`, and `legacy-digest.txt`. These are synthetic checker inputs,
never guest observations or execution receipts. `selftest` exercises admission,
deterministic readback, each verdict class, source mutation and retained-proof
mutation failures. These commands require neither KVM nor a campaign.

The packageable input example is
`crates/chaoscontrol-evidence/fixtures/semantic-history-register.json`.
Invocation `source_artifact` must equal the root source and start with `blake3:`;
real producers must use the retained source artifact's BLAKE3 identity. The
synthetic fixture's explicitly labeled source token is not an artifact receipt.

The pure provider API is `semantic_history::contract::{import_json, export_json,
check_json, validate_json, fixtures, selftest}`. The shell owns filesystem effects;
the checker core remains pure. Existing v1 checker and digest APIs are unchanged.

