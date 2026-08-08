## Why

OnixOS and Mantle can define, build, and transport kernel bundles with exact identities, but shape and digest evidence cannot prove that a selected kernel boots, a ModulePack loads against it, or a BPF Pack passes the verifier and produces its declared observation. Those checks must run away from the host kernel and remain scoped to an exact artifact cohort.

ChaosControl already owns deterministic KVM boot, guest workloads, snapshots, typed evidence, and bounded replay. It should add a dedicated kernel-bundle behavior rail that consumes only admitted Onix/Mantle artifacts, runs boot/module/BPF smoke cases inside disposable VMs, and keeps those results separate from lifecycle-fault replay and physical-hardware claims.

## What Changes

- Add a typed Nickel kernel-bundle campaign profile binding exact Onix bundle, Mantle projection, kernel/initrd, ModulePack, BPF Pack, guest harness, limits, expected observations, and non-claims. r[kernel_bundle_validation.profile]
- Reject stale, incomplete, unsupported, or mismatched Onix/Mantle input evidence before VMM launch. r[kernel_bundle_validation.admission]
- Add bounded boot smoke for supported x86_64 kernel formats with a structured guest readiness oracle. r[kernel_bundle_validation.boot]
- Add disposable guest ModulePack load/observe/unload cases with exact module and kernel identities. r[kernel_bundle_validation.modules]
- Add disposable guest BPF Pack load/attach/trigger/observe/detach cases with exact object, BTF, section, and target identities. r[kernel_bundle_validation.bpf]
- Emit typed redacted behavior receipts and preserve separation from build, lifecycle replay, physical, security, and release evidence. r[kernel_bundle_validation.evidence]
- Add a cheap default wiring rail and an opt-in KVM behavior rail with positive and negative fixtures. r[kernel_bundle_validation.rails] r[kernel_bundle_validation.verification]

## Impact

- **ChaosControl**: gains a new Rust guest harness/profile and evidence class; existing VM replay verdicts and readiness classes remain unchanged.
- **Onix/Mantle**: remain artifact/identity owners and provide exact admitted inputs; ChaosControl does not rebuild or reinterpret bundle semantics.
- **Execution**: all module and BPF loading occurs inside disposable guests, never on the host kernel.
- **Scheduling**: default checks validate wiring/contracts only; KVM behavior remains a dedicated runner lane.
- **Claims**: a pass proves selected operations for one exact VM/kernel/artifact/profile cohort. It does not prove universal bootability, module/eBPF safety, host or physical compatibility, deterministic VMM correctness, deployment readiness, or release eligibility.
