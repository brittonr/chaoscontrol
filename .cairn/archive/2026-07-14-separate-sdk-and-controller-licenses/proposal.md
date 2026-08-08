## Why

ChaosControl combines embeddable guest protocols, SDKs, and workload templates with host-side VMM, exploration, replay, evidence, tracing, and dashboard applications. A single Apache-2.0 workspace license gives controller forks no reciprocity requirement, while a blanket AGPL change would impose strong copyleft concerns on user workloads that link the guest SDK.

## What Changes

- Keep protocol, SDK, guest support, guest examples, and workload templates under Apache-2.0.
- License host-side VMM, fault, trace, exploration, dashboard, replay, and evidence crates under AGPL-3.0-or-later.
- Add complete license texts and a package/path boundary map.
- Update dependency-license policy to accept the project-owned AGPL controller crates.
- Add positive and negative checks for package mapping drift.

## Capabilities

### Added Capabilities
- `license-boundary`: Defines the guest-embedding and host-controller distribution split.

## Impact

Workload authors retain a permissive Rust SDK/protocol surface. Distributed controller and dashboard modifications receive AGPL reciprocity. Runtime behavior and evidence semantics do not change.
