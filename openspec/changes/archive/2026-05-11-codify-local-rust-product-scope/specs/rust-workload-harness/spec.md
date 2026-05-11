## ADDED Requirements

### Requirement: Rust-only SDK scope [r[rust-workload-harness.rust-only-scope]]

The Rust workload harness MUST treat Rust as the only supported SDK language for current product readiness and MUST NOT classify missing Go, Java, Python, C, or other SDKs as blockers for the current local ChaosControl product surface.

#### Scenario: Rust-only docs avoid language-gap framing [r[rust-workload-harness.rust-only-scope.docs]]

- GIVEN the Rust workload harness guide, template, or generated readiness summary is rendered
- WHEN it describes supported SDK scope
- THEN it states that Rust is the supported SDK surface for now
- AND it does not list non-Rust SDKs as active missing features or promotion blockers
