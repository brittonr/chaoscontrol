## Why

Antithesis Rust SDK 0.3.0 resolves catalog registration through its own `linkme` re-export and specifies that crate path to the procedural macro. ChaosControl's exported automatic/stable assertion and guidance registration macros instead resolve `linkme` in the caller. A downstream crate depending only on the SDK fails compilation; the workload template and five guests carry direct dependencies solely to work around this leak.

## Change

Expose the SDK-owned dependency under `full` for macro expansion and qualify every exported catalog registration through `$crate`, including linkme's explicit crate-path override. Remove the obsolete direct dependencies from all five guest manifests and the workload template; regenerate their lockfile projections through Cargo. Update current authoring guidance. Preserve catalog identities, declaration/hit semantics, source metadata, guidance watermarks and no-full behavior.

## Consumer and ownership

Consumers: SDK-only downstream Rust workloads, renamed SDK dependencies, the copyable workload template and the five instrumented guests. Owner: ChaosControl SDK/workload maintainers. Immediate outcome: consumers need not mirror the SDK's registration dependency. Durable contribution: self-contained exported macros, continuously exercised by workspace guests without direct `linkme`, plus repeatable external/template smoke evidence. No new dependency, runtime allocation, protocol change, VM/replay or hosted-platform claim.
