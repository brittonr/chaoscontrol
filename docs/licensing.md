# ChaosControl licensing boundary

## Apache-2.0 guest and embedding surfaces

- `crates/chaoscontrol-protocol/**`
- `crates/chaoscontrol-sdk/**`
- `crates/chaoscontrol-guest/**`
- `crates/chaoscontrol-guest-net/**`
- `crates/chaoscontrol-raft-guest/**`
- `crates/chaoscontrol-net-guest/**`
- `crates/chaoscontrol-redb-guest/**`
- `crates/chaoscontrol-rust-workload-guest/**`
- Repository-owned workload templates and scaffold output intended to be copied into downstream projects

## AGPL-3.0-or-later host and controller surfaces

- `crates/chaoscontrol-fault/**`
- `crates/chaoscontrol-vmm/**`
- `crates/chaoscontrol-trace/**`
- `crates/chaoscontrol-explore/**`
- `crates/chaoscontrol-dashboard/**`
- `crates/chaoscontrol-replay/**`
- `crates/chaoscontrol-evidence/**`
- Repository documentation and lifecycle material not carrying another notice

Apache guest/SDK packages must not depend on AGPL host packages. Host packages may depend on Apache packages. Third-party dependencies, kernels, web assets, generated material containing upstream content, and external workloads retain their own terms.

Complete texts are in `LICENSES/Apache-2.0.txt` and `LICENSES/AGPL-3.0-or-later.txt`.

Processing a workload does not by itself relicense unrelated workload source or output. License metadata is outside VM, snapshot, replay, and evidence identity unless a versioned schema explicitly includes it. Earlier grants remain valid; this map does not establish legal compliance or global determinism.
