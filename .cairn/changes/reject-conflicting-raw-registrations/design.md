## Context

`register_raw` returns true whenever a raw descriptor shares a compatibility alias with an earlier registration. Later resolution checks alias, kind and message, so a different string key with the same alias/kind/message can become a hit on the first property. The fallback writer then emits the first property's ID and metadata, hiding the conflict.

## Decisions

### Decision: Exact existing-registration admission

When an alias is already registered, compare the complete descriptor plus class, function, display type and must-hit metadata. Return true only for an exact match. The descriptor already binds string key, kind, message, source file/line/column, namespace, guest and category. Compare borrowed strings without further allocation. Preserve the current lock and registration lifetime.

### Decision: Reject, do not invent alternate collision routing

The raw API currently uses alias-based lookup and one raw registration per alias. Keep that bounded contract: a colliding ID is rejected rather than accepted under another property's identity. Do not add alternate alias assignment, shims, telemetry or change protocol catalog rules. The accepted original property remains usable. Rejected declarations/hits cannot reach sparse sampling. Raw IDs therefore still have a stricter collision-admission limit than upstream string IDs; this is explicit in documentation.

## Risks / Trade-offs

Adapters that change declaration metadata on hits now have those hits rejected instead of silently rewritten. Document exact reuse requirements; regenerate evidence from affected runs. Tests cover a known fixed alias collision, each identity/source/semantic metadata change, exact duplicates, genuine positive/negative outcomes, late unknown sites and default/sparse emission. Strict CLI smoke proves rejected false observations do not taint a passing property while a genuine false hit still fails. No VM/replay claim is made.
