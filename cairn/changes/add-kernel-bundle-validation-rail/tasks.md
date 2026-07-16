## Phase 1: Profile and upstream admission

- [x] [depends:onixos.define-onix-kernel-bundle-v1] Add exact canonical Onix bundle/pack manifest and validation receipt fixtures. r[kernel_bundle_validation.profile] r[kernel_bundle_validation.admission]
  - Evidence: `cairn/changes/add-kernel-bundle-validation-rail/evidence/mantle-private-kfunc-onix-validation-2026-07-15.json` and `cairn/changes/add-kernel-bundle-validation-rail/evidence/mantle-private-kfunc-progress-2026-07-15.md` bind the accepted Onix kernel/module/BPF/bundle identities for the Mantle private-kfunc cohort.
- [x] [depends:mantle.add-onix-kernel-bundle-oci-projections] Add exact Mantle OCI/materialization receipt fixtures and object-ref reconstruction inputs. r[kernel_bundle_validation.admission]
  - Evidence: `evidence/mantle-private-kfunc-materialization-2026-07-15.json` binds the exact module/BPF/loader object identities and initrd reconstruction paths without promoting the fixture to authority.
- [x] [serial] Define the typed Nickel campaign profile, supported boot cohort, module/BPF cases, structured observations, named bounds, retention, and non-claims. r[kernel_bundle_validation.profile]
  - Evidence: `crates/chaoscontrol-evidence/src/kernel_bundle_validation.rs` and `evidence/mantle-private-kfunc-profile-2026-07-15.json` define the bounded profile and exact supported cohort.
- [x] [serial] Implement pure upstream evidence, identity-role, artifact availability, boot-format, pack-target, and guest-injection admission. r[kernel_bundle_validation.admission]
  - Evidence: `chaoscontrol_evidence::kernel_bundle_validation::validate_profile` rejects role drift, stale/missing upstream evidence, mismatched identities, missing observations, and cleanup/non-claim gaps before receipt projection.

## Phase 2: Guest harness and boot control

- [x] [serial] Implement the bounded Rust guest case protocol and setup/boot/module/BPF/cleanup observation DTOs without raw-log verdicts. r[kernel_bundle_validation.boot] r[kernel_bundle_validation.evidence]
  - Evidence: `KernelBundleKvmObservation`, `extract_kvm_observations`, and the `chaoscontrol-kernel-bundle:v1:` marker protocol in `crates/chaoscontrol-evidence/src/kernel_bundle_validation.rs`; `kernel-bundle-vm-compat-smoke --check-kvm-serial` classifies only structured markers.
- [x] [serial] Implement supported-format kernel/initrd injection, deterministic VMM configuration, no-fault boot control, and structured readiness oracle. r[kernel_bundle_validation.boot]
  - Evidence: `kernel-bundle-vm-compat-smoke --kvm-run-profile <profile> --kernel <path> --initrd <path> --out <receipt>` wraps `chaoscontrol_vmm::DeterministicVm` and emits blocked receipts when KVM or loader inputs are unavailable.
- [x] [parallel] Add positive boot fixtures and negative unsupported format, stale kernel/initrd, release/architecture mismatch, no readiness, panic, bound, and raw-log-only fixtures. r[kernel_bundle_validation.verification]
  - Evidence: `kernel_bundle_validation` tests cover unsupported profile facts, stale image bindings, no readiness, panic, VM-exit bounds, and raw transcript rejection; exact positive and stale-digest receipts are retained under `evidence/`.

## Phase 3: Module and BPF behavior

- [x] [serial] Implement read-only ModulePack injection plus load/identity/observation/unload/cleanup case execution inside disposable guests. r[kernel_bundle_validation.modules]
  - Evidence: `kernel_bundle_initrd::write_private_kfunc_initrd` injects the exact Mantle module, the KVM run loaded and unloaded it, and `evidence/mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json` records module load/unload/cleanup observations.
- [x] [serial] Implement read-only BPF Pack injection plus byte/BTF/manifest verification, load/attach/trigger/observe/detach/cleanup case execution inside disposable guests. r[kernel_bundle_validation.bpf]
  - Evidence: the repo-owned initrd injects `private_kfunc.ebpf.o`, uses bpftool for verifier admission, runs the exact `private_kfunc` loader against `lo`, and records verify/attach/detach/cleanup in `evidence/mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json`.
- [x] [parallel] Add positive fixture module and BPF cases with exact expected observations. r[kernel_bundle_validation.modules] r[kernel_bundle_validation.bpf] r[kernel_bundle_validation.verification]
  - Evidence: `evidence/mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json` has `status: passed`, `execution_mode: chaoscontrol-vmm-kvm`, digest-bound kernel/initrd inputs, and no issues.
- [x] [parallel] Add negative target mismatch, tampered member, vermagic/signature observation, module rejection/taint/unload failure, absent BTF, missing kfunc/type, verifier rejection, wrong attach target, missing event, and cleanup failure cases. r[kernel_bundle_validation.verification]
  - Evidence: the pure negative matrix covers every listed class; selected exact KVM receipts retain missing-kfunc, verifier-rejection, wrong-attach-target, and cleanup-failure outcomes with `negative_fixture_matched = true`.

## Phase 4: Evidence and rails

- [x] [serial] Add pure behavior classification and redacted `kernel-bundle/vm-compat-smoke` receipts binding all Onix/Mantle/ChaosControl identities, bounds, observations, and cleanup. r[kernel_bundle_validation.evidence]
  - Evidence: `evidence/mantle-private-kfunc-vm-compat-smoke-2026-07-15.json` has receipt role `kernel-bundle/vm-compat-smoke` and identity `fb37d05d6ee328b05d8f1bdc80ae0d622dcdef590f0dbf7e2721bb3993e76119`.
- [x] [parallel] Add the cheap default profile/fixture/adapter/protocol/non-claim wiring rail with guards against behavior-proof promotion. r[kernel_bundle_validation.rails]
  - Evidence: `cargo test -p chaoscontrol-evidence kernel_bundle_validation` passed with positive and negative tests for exact admission, role confusion, stale inputs, cleanup gaps, and non-claim gaps.
- [x] [serial] Add the dedicated KVM rail for selected boot, module, and BPF positive/negative cases; missing KVM or loaders must report blocked. r[kernel_bundle_validation.rails]
  - Evidence: `evidence/kvm-rail-validation-2026-07-15.md` records exact positive KVM execution, four exact guest negatives, stale-digest prelaunch blocking, transcript rejection, and unavailable-input blocking.
- [x] [parallel] Add guards proving these receipts cannot satisfy snapshot replay, Onix lifecycle replay, physical readiness, build correctness, security, or release gates. r[kernel_bundle_validation.evidence] r[kernel_bundle_validation.verification]
  - Evidence: `kernel_bundle_receipt_supports_use` accepts only exact issue-free positive VM compatibility smoke and denies every broader role in focused tests.

## Phase 5: Documentation and closeout

- [x] [parallel] Document supported cohorts, reproduction, guest loaders, case authoring, artifact retention, prerequisites, evidence classes, and non-claims. r[kernel_bundle_validation.rails]
  - Evidence: `docs/kernel-bundle-validation.md` and the README entry document the full bounded rail.
- [x] [serial] Run focused pure-core, contract, guest, wiring, and KVM behavior checks plus formatting, clippy, and dependency policy. r[kernel_bundle_validation.verification]
  - Evidence: focused formatting, 13 validation tests, 4 initrd tests, exact positive/negative KVM scenarios, and first-party clippy passed; Nix dependency audit and dependency policy checks passed in pueue task 25.
- [x] [serial] Run Cairn validation and proposal/design/tasks gates; sync and archive only with at least one current bounded boot, module, and BPF behavior receipt plus all negative fixtures. r[kernel_bundle_validation.verification]
  - Evidence: canonical-policy validate plus proposal/design/tasks gates passed in pueue task 19 with 16 completed tasks before these final two boxes were closed. The persisted exact positive KVM receipt and five negative receipts are indexed in `evidence/kvm-rail-validation-2026-07-15.md`.
