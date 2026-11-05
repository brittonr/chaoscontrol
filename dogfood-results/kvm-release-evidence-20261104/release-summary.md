# KVM Release Matrix Summary

- Terminal class: `ReleaseEligible`
- Source revision: `901b697ae601e686424c1635d7dc51c2fea4af37`
- Matrix: `blake3:e9c5ff6dd088cc59159aaa057e78de5cb9cd73d0e220ea6c80e956db6a86d9e0`
- Worker: `x86_64` / `6.18.50`

## Rows

| Row | Kind | Status | Artifacts |
| --- | --- | --- | ---: |
| `build-release-binaries` | `CohortBinaryBuild` | `Passed` | 88 |
| `build-raft-guest` | `CohortGuestBuild` | `Passed` | 3 |
| `deterministic-smp` | `DeterministicSmp` | `Passed` | 2 |
| `serialized-snapshot-replay` | `SerializedSnapshotReplay` | `Passed` | 2 |
| `virtio-malformed-input` | `VirtioMalformedInput` | `Passed` | 2 |
| `admitted-drift` | `AdmittedDrift` | `Passed` | 9 |
| `fresh-workload-replay` | `FreshWorkloadReplay` | `Passed` | 103 |

## Blockers

None.

## Claim Boundary

This receipt establishes that the exact source revision passed the required matrix on the recorded admitted worker with the recorded commands, bounds, and artifact identities.
- The receipt does not prove worker integrity.
- The receipt does not prove all-host equivalence.
- The receipt does not prove universal determinism.
- The receipt does not prove workload correctness.
- The receipt does not prove production availability.
