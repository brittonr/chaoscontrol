## Tasks

- [x] [serial] Enumerate the current deterministic surfaces and the guest reads they do not cover. r[chaoscontrol.guest_determinism.boundary]
- [x] [depends:guest-determinism-baseline] Inject a run-derived Linux boot seed, bind its derivation, and bound CRNG replay to an admitted quiescent snapshot. r[chaoscontrol.guest_determinism.boot_entropy]
- [x] [depends:guest-determinism-entropy] Select deterministic jiffies from the VMM timer plan and record the clock profile. r[chaoscontrol.guest_determinism.time_surface]
- [x] [depends:guest-determinism-time] Apply the fixed Linux layout policy and record its run-bound identity in evidence. r[chaoscontrol.guest_determinism.layout]
- [x] [depends:guest-determinism-layout] Verify signal delivery order derives from the deterministic schedule. r[chaoscontrol.guest_determinism.signals]
- [x] [depends:guest-determinism-signals] Add the snapshot-backed bit-exact validation fixture guest and wire the drift gate. r[chaoscontrol.guest_determinism.validation_fixture]
- [x] [parallel] Add negative fixtures for entropy, clock, layout, and signal-order drift. r[chaoscontrol.guest_determinism.validation]
- [x] [depends:guest-determinism-validation] Run focused VM, replay, evidence, Cairn, and relevant Nix validation. r[chaoscontrol.guest_determinism.validation]
