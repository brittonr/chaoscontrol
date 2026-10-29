## Why

Antithesis 0.3.0 documents setup completion as first-call-wins: repeated calls have no effect on the setup transition. Its SDK still emits each call, as does ChaosControl's SDK. Both ChaosControl local readers currently reject repeated setup records, so a valid workload cannot reach a report after multiple services call `setup_complete`.

## Change

Make strict evidence and SDK diagnostic readers retain only the first validated completion and its adoption metadata. Continue validating every later setup record before treating it as idempotent: malformed records and resource violations must not disappear behind completed state. Keep one semantic setup transition, assertion identity/counters, downstream events and evidence classification unchanged. Align the SDK diagnostic reader's details admission with the already supported arbitrary-JSON SDK/strict-reader contract.

## Consumer and ownership

Consumer: Rust workloads with multiple initialization components calling `setup_complete`, plus their local report tooling. Existing callers adopt reader updates unchanged. Owner: ChaosControl SDK/evidence maintainers. Immediate outcome: repeated setup output reaches valid reports without metadata replacement. Durable capability: consistent first-wins lifecycle reduction, with positive and adversarial regression coverage. No SDK output suppression, VM transition change, cross-process global ordering, replay, hosted-platform or release-readiness claim.
