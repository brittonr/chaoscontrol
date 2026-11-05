## Why

Antithesis-compatible assertion records carry source location metadata. ChaosControl's local bound writer currently recovers that metadata through compact alias, kind and message rather than the already accepted fingerprint. Distinct stable sites in different namespaces can share all three, causing hits to lose file/line/class/function and declarations to lose class/function.

## What changes

Resolve hit locations from the accepted descriptor selected by fingerprint. Resolve declarations directly from their descriptor. Match compiled source metadata by namespace, logical key, source coordinates, kind, message and alias; match raw registrations by their exact descriptor. Remove the obsolete alias-only location lookup rather than retaining a fallback. Leave JSON fields, catalog admission, VM transport and verdict semantics unchanged.

## Consumer, ownership and adoption

Consumer: local assertion/fallback tooling and triage of Rust workloads with stable sites. Owner: ChaosControl SDK maintainers. Existing callers need no changes. Immediate outcome: colliding admitted sites retain their own function/file metadata. Durable capability: identity-bound source attribution with a behavioral collision regression and existing raw/fallback coverage.

This is local instrumentation parity, not hosted parity, VM execution, replay, whole-program correctness or release eligibility.
