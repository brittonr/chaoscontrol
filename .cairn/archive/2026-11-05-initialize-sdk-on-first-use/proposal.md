## Why

Unlike Antithesis 0.3.0's lazy output/random handler, lifecycle-only, randomness-only and direct-guidance consumers currently skip SDK initialization. Their local output can omit SDK metadata and declarations for unexecuted must-hit assertions. The SDK already promises lazy initialization, but only assertion hits enforce it.

## What changes

Initialize before public setup/event output, entropy-consuming random requests (including index choice), and direct guidance hits. Keep explicit initialization idempotent. Separate the internal initialization-time event writer from the public initializing entry point so metadata, must-hit metadata and guidance declarations cannot recursively enter the initialization lock.

## Consumer and ownership

Consumer: Rust workloads that begin with lifecycle milestones, random decisions or direct guidance without calling `chaoscontrol_init`. Owner: ChaosControl SDK maintainers. Existing callers adopt the behavior unchanged. Immediate outcome: metadata and the closed assertion catalog precede first-use observations. Durable capability: consistent one-time initialization across instrumentation entry points, with isolated-process and concurrent first-use regressions.

## Boundaries

No new transport, runtime protocol, identity model, hosted service or language SDK. Empty/singleton choice and empty byte-buffer operations still avoid entropy requests. No-full builds remain no-op. Initializing through these APIs closes raw catalog registration just as explicit initialization already does. Local output evidence does not establish VM execution, replay or release eligibility.
