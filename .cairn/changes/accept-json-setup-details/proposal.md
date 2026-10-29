## Why

Antithesis 0.3.0 `lifecycle::setup_complete(&serde_json::Value)` accepts arbitrary JSON details, and ChaosControl's SDK already writes those values unchanged. The local evidence reader instead deserializes setup details as an object, rejecting valid SDK output containing null, booleans, numbers, strings or arrays. This breaks the producer-to-report path despite the matching public SDK signature.

## Change

Accept every JSON details value in local setup records. Retain required record fields, complete status, single-setup semantics, object metadata field/key bounds and generic JSON stream resource bounds. Extract adoption metadata only from a top-level object. Keep assertion identity, counters and local-only evidence boundaries unchanged.

## Consumer and ownership

Current consumer: Rust workloads and external Rust adapters using `setup_complete` with scalar/array/null details. Outcome: their actual SDK output can reach strict local reports and quality gates. Existing callers adopt the reader fix unchanged. Owner: ChaosControl evidence maintainers. Durable capability: SDK-compatible lifecycle evidence admission with regression coverage. This does not prove VM execution, replay, hosted platform parity or release eligibility.
