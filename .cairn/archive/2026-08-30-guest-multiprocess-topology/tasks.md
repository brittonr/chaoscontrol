## Tasks

- [x] [serial] Confirm the single-process init path, VM-targeted faults, and single-page SDK transport as the baseline. r[chaoscontrol.guest_processes.manifest] r[chaoscontrol.guest_processes.transport_isolation]
- [x] [depends:multiprocess-baseline] Add a Nickel-owned process manifest contract and a pure admission and projection core. r[chaoscontrol.guest_processes.manifest]
- [x] [depends:multiprocess-manifest] Implement the deterministic guest supervisor with spawn, monitor, restart, and lifecycle events. r[chaoscontrol.guest_processes.supervisor]
- [x] [depends:multiprocess-supervisor] Expose one shared deterministic device surface for declared working directories. r[chaoscontrol.guest_processes.shared_storage]
- [x] [depends:multiprocess-storage] Add host-directed process faults with role targeting and typed rejection. r[chaoscontrol.guest_processes.process_faults]
- [x] [depends:multiprocess-faults] Isolate per-process SDK transport and extend the oracle and evidence identity with process scope. r[chaoscontrol.guest_processes.transport_isolation] r[chaoscontrol.guest_processes.evidence_scope]
- [x] [parallel] Add positive cooperating-process fixtures and negative crash, restart, invalid-target, corruption, and state-loss fixtures. r[chaoscontrol.guest_processes.validation]
- [x] [depends:multiprocess-validation] Run focused SDK, VM, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.guest_processes.validation]
