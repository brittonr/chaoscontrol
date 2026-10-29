## Context

The fallback schema uses a single `reachability` assert type, but ChaosControl's protocol has separate reachable/unreachable kinds. `AssertType::to_kind(condition)` is currently used by `assert_raw` even for catalog-only declarations. False is a valid placeholder condition for a reachable declaration, so condition-dependent classification changes the descriptor between registration and execution.

## Decisions

### Decision: Classify by execution obligation

Change the existing `AssertType::to_kind` boolean contract to `must_hit`: true selects reachable, false selects unreachable. Always/sometimes classification remains independent of the boolean. Update the sole runtime caller and documentation. Existing tests passing true/false already expect that same kind mapping; retain their assertions, and add behavior that distinguishes obligations from conditions.

### Decision: Preserve transport and admission

No new API, schema, compatibility shim or alternate identity path. The emitted condition remains the caller's condition; only the concrete kind is selected from stable property metadata. Existing sparse first-pass/first-fail sampling and late-site rejection remain in force. Regenerate previous raw reachable artifacts that were misclassified; do not reinterpret them as current evidence.

## Risks / Trade-offs

External callers of `to_kind` must now pass the execution obligation, not an observation condition. The old use was incapable of classifying declarations correctly. Declaration/hit regression tests cover required and optional reachability, preserved source/identity, late rejection and sparse behavior; strict report CLI scenarios distinguish required quiet, successful required and forbidden-path violation outcomes. No KVM claim is made without a VM run.
