## Decision

Resolve an accepted bound identity before consulting sparse tracking. Key the existing process-local map by the canonical descriptor fingerprint rather than compact alias and assertion kind. Fingerprints already include the kind and namespace. Store two observed-outcome bits rather than unused counters, so sampling cannot overflow after repeated evaluations. Resolve failures do not create sampling entries. The closed admitted catalog bounds the map's identity set.

Stable and compatibility entry points share tracking. Repeated evaluations are sampled before details serialization, preserving existing sparse-mode cost behavior. Catalog declarations remain outside tracking. Default mode still emits every hit. Serialization failure behavior is unchanged and is not expanded in this change.

## Verification

Use two admitted stable descriptors in different namespaces with a fixed FNV alias collision. Interleave stable and compatibility calls, checking each fingerprint has exactly one true and one false hit, accepted identity, and an unsampled declaration. Confirm default mode retains every evaluation and no-full call shapes still compile. Run an actual local output smoke; no KVM is required for this SDK-only correction.
