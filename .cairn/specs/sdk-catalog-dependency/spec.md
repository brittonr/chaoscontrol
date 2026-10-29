# Sdk Catalog Dependency Specification

## Purpose

Defines the `sdk-catalog-dependency` capability.

## Requirements

### Requirement: SDK catalog registration owns its dependency

r[chaoscontrol.sdk_catalog_dependency.encapsulation]

Downstream automatic assertion, stable assertion and guidance macros MUST compile and register through the SDK-owned catalog dependency without a direct caller dependency on `linkme`. Resolution MUST remain correct when the SDK dependency is renamed. The SDK MUST retain its existing accepted catalog identity, source metadata, declarations, assertion outcomes, optional must-hit behavior and guidance records. No-full builds MUST remain dependency-free with respect to catalog registration, preserve input evaluation and emit no output. Messages MUST remain const-evaluable.

Current guest manifests and the workload template MUST remove direct registration dependencies made obsolete by this change. Lockfiles MUST reflect the manifests while retaining the SDK's own dependency. This requirement establishes local macro/consumer behavior, not VM execution or replay correctness.

#### Scenario: A consumer depends only on the SDK

- GIVEN automatic, stable and guidance sites in a downstream crate whose only instrumentation dependency is the SDK, optionally renamed
- WHEN the consumer builds and runs with full enabled
- THEN its assertion and guidance declarations and observations retain their existing identities and semantics without caller-owned linkme

#### Scenario: Instrumentation is disabled or a message is invalid

- GIVEN the same consumer without full, or a runtime-computed assertion message
- WHEN the consumer builds/runs
- THEN no-full calls preserve input evaluation without output and runtime messages are rejected at compile time
