## Tasks

- [x] [serial] Confirm which resource and clock capabilities return unsupported rejections today. r[chaoscontrol.fault_surface.unsupported_visible]
- [ ] [depends:fault-surface-baseline] Implement clock-freeze effect execution against the virtual clock. r[chaoscontrol.fault_surface.clock_freeze]
- [ ] [depends:fault-surface-freeze] Implement bounded clock-jitter effect execution. r[chaoscontrol.fault_surface.clock_jitter]
- [ ] [depends:fault-surface-jitter] Implement vCPU stall using the existing stall plumbing with exact release. r[chaoscontrol.fault_surface.cpu_stall]
- [ ] [depends:fault-surface-stall] Implement the deterministic guest-visible memory ceiling with baseline release. r[chaoscontrol.fault_surface.memory_pressure]
- [ ] [depends:fault-surface-memory] Route all new effects through the six-stage ledger with typed rejection records. r[chaoscontrol.fault_surface.stage_evidence]
- [ ] [parallel] Add positive freeze, jitter, stall, and pressure fixtures and negative window, release, stage, and profile fixtures. r[chaoscontrol.fault_surface.validation]
- [ ] [depends:fault-surface-validation] Run focused planner, VM, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.fault_surface.validation]
