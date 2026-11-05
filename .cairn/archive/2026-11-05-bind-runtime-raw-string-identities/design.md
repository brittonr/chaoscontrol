## Context

Raw runtime descriptors currently carry `Some(location_id(id))`, register by alias and emit through compatibility resolution. Protocol descriptors already allow `compatibility_id=None`; stable-key lookup and bound transport already support it. There is no need for alternate alias assignment or a new protocol.

## Decisions

### Decision: Runtime raw IDs are stable logical keys without aliases

Set runtime raw descriptor compatibility IDs to None. Key registration reuse by namespace/logical key, retaining complete immutable metadata checks. Borrow the incoming descriptor for lookup and clone it only for first registration; repeated hits do not copy the descriptor into storage. Retain it in the caller long enough to borrow the exact string key for stable resolution.

### Decision: Share the stable emission core

Extract the existing stable assertion emitter's implementation into a private core with an extra-flags argument. Existing stable APIs call it with zero extra flags; raw optional always calls supply the existing AOU flag. Identity admission, fingerprint-based sparse sampling, bounded JSON and bound transport remain common. Delete the old raw alias precheck and compatibility-emission branch. Compiled macro aliases and public API signatures remain unchanged.

### Decision: Preserve immutable same-ID registration

Update the existing raw-registration requirement: distinct keys are independent, not metadata conflicts. Same-ID changes remain rejected before emission/sampling. Migrate the old collision-rejection regression to cover same-ID conflicts, and add a fixed-collision independence regression. Unknown late keys remain rejected.

## Risks / Trade-offs

Runtime raw descriptor fingerprints/catalog tokens change; regenerate artifacts instead of adding an alias fallback. Alias-free VM transport is existing code, but only local instrumentation is exercised here and no VM/replay claim follows. Fixed-collision regressions check catalog membership, identity, per-ID default/sparse counts, raw IDs and forbidden late calls. Real SDK-to-strict-report scenarios must accept both passing properties and reject a genuine violation of either without merging counts. Existing optional-always and reachability regressions cover shared emitter flags and semantics.
