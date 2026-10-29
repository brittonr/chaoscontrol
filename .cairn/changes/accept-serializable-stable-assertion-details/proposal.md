## Why

Antithesis Rust SDK 0.3.0 accepts borrowed `serde::Serialize` assertion details. ChaosControl's automatic and optional assertions already do, but its stable-identity macros route through `assert_raw_with_key`, which still accepts only `serde_json::Value`. Rust workload authors therefore must allocate an intermediate JSON value solely to keep a stable assertion identity.

## What changes

Make the full-mode stable emitter generic over `Serialize + ?Sized`, reusing its existing bounded serializer after identity admission and sparse sampling. Preserve all `Value` callers and the existing unconstrained borrowed no-full API. Exercise borrowed structs, unsized strings/slices, positive/negative outcomes, lazy sparse serialization and unknown-site rejection. No new dependency or intermediate conversion.

## Capability and boundaries

Owner: ChaosControl SDK maintainers. Current consumer: Rust workload authors using stable assertions with typed diagnostic structs. Immediate outcome: the same borrowed details work with automatic and stable macros. Durable contribution: one bounded serialization contract across those paths, protected by runtime regressions and an external full/no-full consumer. This extends Antithesis's typed-details convention to ChaosControl's own stable-identity extension; it does not claim an upstream stable API, VM execution, replay or hosted-platform parity.
