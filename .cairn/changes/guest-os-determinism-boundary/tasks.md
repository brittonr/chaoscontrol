## Tasks

- [x] [serial] Enumerate the current deterministic surfaces and the guest reads they do not cover. r[chaoscontrol.guest_determinism.boundary]
- [ ] [depends:guest-determinism-baseline] Inject deterministic boot-time entropy from the run seed and record the derivation. r[chaoscontrol.guest_determinism.boot_entropy]
- [ ] [depends:guest-determinism-entropy] Pin guest clocks to the virtual TSC and record the clock profile. r[chaoscontrol.guest_determinism.time_surface]
- [ ] [depends:guest-determinism-time] Derive ASLR and layout seeds from run configuration and record them in evidence. r[chaoscontrol.guest_determinism.layout]
- [ ] [depends:guest-determinism-layout] Verify signal delivery order derives from the deterministic schedule. r[chaoscontrol.guest_determinism.signals]
- [ ] [depends:guest-determinism-signals] Add the bit-exact validation fixture guest and wire the drift gate. r[chaoscontrol.guest_determinism.validation_fixture]
- [ ] [parallel] Add negative fixtures for entropy, clock, layout, and signal-order drift. r[chaoscontrol.guest_determinism.validation]
- [ ] [depends:guest-determinism-validation] Run focused VM, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.guest_determinism.validation]
