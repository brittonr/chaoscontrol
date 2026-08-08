## Why

`chaoscontrol-sdk` deliberately exposes an Antithesis-style API, but the
project has no tracked record of how each SDK surface maps to the other.
The Antithesis Rust SDK is a comparison source with guidance features
(numeric and boolean watermark reporting) that ChaosControl does not
implement. Without a tracked parity map, the project cannot tell which
surfaces match and which differ, and the guidance gap has no recorded
decision.

## What Changes

- Add a versioned parity mapping document under `docs/references/` that
  maps every `chaoscontrol-sdk` surface to the `antithesis_sdk` 0.2.9
  surface, marks each entry equivalent, superset, subset, divergent, or
  absent, and records the local-output schema relationship.
- Record a decision on guidance watermarks: they are not a current
  requirement, with rationale, kept open for future explorer work.
- Track the parity mapping and the guidance decision as a native Cairn
  change with requirement IDs.

## Impact

- **Files**: `docs/references/sdk-antithesis-rust-parity.md`;
  `.cairn/changes/sdk-parity-and-guidance-watermarks/{proposal,design,tasks}.md`;
  `.cairn/changes/sdk-parity-and-guidance-watermarks/specs/sdk-parity-and-guidance/spec.md`
- **Testing**: Cairn validation and proposal gate; no product behavior
  changes, so no Rust checks are required.
- **Claims**: The mapping is a comparison aid, not a parity requirement.
  It does not change SDK behavior, version numbers, or the Antithesis
  non-goal boundary.
