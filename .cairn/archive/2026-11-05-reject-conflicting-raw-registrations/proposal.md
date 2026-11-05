## Why

The Antithesis raw adapter surface identifies properties by their caller-supplied string ID and declaration metadata. ChaosControl preserves those IDs in output but currently reuses runtime registrations solely by a 32-bit compatibility alias. Distinct colliding raw IDs with the same kind/message can therefore emit under the first property's identity. Changed source, display or must-hit metadata can also be silently attributed to the original registration.

## What Changes

Require an exact descriptor and registration-metadata match before reusing an existing raw alias. Reject conflicts before identity resolution, serialization or sparse sampling; keep the existing declaration and accepted catalog untouched. Preserve exact duplicate registration, legitimate passing/failing observations and late unknown-site rejection.

## Impact

Owner: ChaosControl SDK maintainers. Current consumer: third-party raw assertion adapters. Immediate outcome: rejected raw observations cannot change another property's counts or consume its sparse samples. Durable capability: exact registration reuse with fixed-collision and metadata-conflict regressions plus actual SDK-to-strict-CLI checks. Public APIs, output schemas and VM commands are unchanged. This is bounded local identity correctness, not full upstream ID-capacity parity, VM proof, replay or release readiness.
