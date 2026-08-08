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

## Checker verdicts

The checker derives real-time precedence from invocation and completion intervals. It evaluates pure register and compare-and-swap models.

The checker returns one of these verdicts:

- `valid`: A retained witness gives one legal linearization.
- `invalid`: No legal model transition exists in the admitted finite search.
- `unknown`: Pending evidence, an unsupported model, or a declared bound stopped evaluation.

A bound failure cannot produce `valid`.

The profile limits operation count, states, branches, depth, memo bytes, and reduction attempts. Independent-key decomposition needs an explicit key-isolation property from the model.

## Invalid witnesses

An invalid report retains the failed operation set and model-state identity. The reducer removes paired operations only. It reports `locally_reduced` or `budget_limited`.

A `budget_limited` result is not minimal.

## Reference checker boundary

The Jepsen-compatible adapter imports and exports typed operation events. The external reference checker is not part of the pure core.

A conformance report binds both tool identities and both verdicts. A disagreement blocks promotion. It does not select one checker as authoritative.

The external tool is optional outside the conformance rail. It is not a runtime dependency for ChaosControl.

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
