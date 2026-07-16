# Kernel Bundle Validation Specification

## Purpose

Defines the `kernel-bundle-validation` capability.

## Requirements

### Requirement: Kernel-bundle campaigns are typed and bounded

r[kernel_bundle_validation.profile] ChaosControl MUST define a typed Nickel kernel-bundle campaign profile binding exact Onix bundle/kernel/manifest/pack identities, Mantle projection/materialization evidence, guest harness/rootfs/initrd identity, supported boot format, command line, module/BPF cases, expected observations, deterministic seed, named execution/artifact bounds, retention, and non-claims.

#### Scenario: Complete profile is admitted

- GIVEN a profile names one supported exact artifact and harness cohort with positive controls, expected cases, finite bounds, and non-claims
- WHEN profile validation runs
- THEN it MAY proceed to upstream artifact admission
- AND every later observation MUST remain bound to that profile identity.

#### Scenario: Profile is incomplete or unbounded

- GIVEN a profile omits an artifact identity, expected observation, cleanup policy, finite bound, or evidence scope
- WHEN validation runs
- THEN it MUST fail before VMM launch.

### Requirement: Upstream evidence gates VMM launch

r[kernel_bundle_validation.admission] ChaosControl MUST verify the canonical Onix manifest/validation receipt, matching Mantle OCI or materialization receipt, component and pack availability, digest roles, target bindings, supported boot format, and guest injection plan before creating a VM.

#### Scenario: Exact admitted inputs pass

- GIVEN every Onix BLAKE3 role, required OCI SHA-256 role, Mantle object ref, pack target, and local artifact byte identity agrees
- WHEN input admission runs
- THEN the campaign MAY construct the bounded VMM plan
- AND ChaosControl MUST retain the upstream identities without reinterpreting their build claims.

#### Scenario: Input evidence is stale or mismatched

- GIVEN any manifest, receipt, object, component, pack, architecture, kernel release, BTF, or boot-format fact is missing, stale, unsupported, or mismatched
- WHEN admission runs
- THEN the campaign MUST stop before VMM creation
- AND runtime smoke MUST NOT fill the missing build, provenance, or transport evidence.

### Requirement: Kernel boot uses a structured readiness oracle

r[kernel_bundle_validation.boot] The initial rail MUST boot only admitted supported x86_64 kernel formats under a deterministic bounded VMM profile and MUST require a guest-owned structured readiness observation matching the expected architecture, kernel release, profile, and case identities.

#### Scenario: No-fault boot control becomes ready

- GIVEN a supported exact kernel/initrd and guest harness are admitted
- WHEN the no-fault control runs within its named bounds
- THEN the harness MUST report setup completion and bounded readiness for the expected kernel/profile identities
- AND raw serial text alone MUST NOT satisfy boot success.

#### Scenario: Kernel does not reach readiness

- GIVEN the guest panics, halts, reports another release/architecture, emits only raw logs, or exceeds a virtual-step/exit/memory bound
- WHEN boot classification runs
- THEN it MUST emit a typed boot failure or bound result rather than a pass.

### Requirement: ModulePacks are exercised inside disposable guests

r[kernel_bundle_validation.modules] Each required ModulePack case MUST verify exact injected member bytes and target facts, load the declared module inside a disposable guest, collect the declared bounded observation, classify taint/signature/load state, and unload or record an explicit cleanup outcome.

#### Scenario: Module case matches

- GIVEN an exact compatible module member loads and produces its declared safe observation under the selected kernel
- WHEN the module case completes
- THEN evidence MUST bind pack/member/kernel/case identities and the matched observation
- AND successful unload/cleanup MUST be recorded separately from load success.

#### Scenario: Module is rejected or cannot clean up

- GIVEN member bytes or target facts differ, the kernel rejects the module, the observation is absent, taint/signature policy is not satisfied, or unload/cleanup fails
- WHEN module classification runs
- THEN the required case MUST fail or remain explicitly unsupported
- AND a fresh disposable VM MUST prevent the failed case from contaminating later cases.

### Requirement: BPF Packs are exercised through declared attach cases

r[kernel_bundle_validation.bpf] Each required BPF Pack case MUST verify exact object, manifest, BTF, section, attach class/target, kfunc/type requirements, trigger, and expected event identities, then load, attach, trigger, observe, detach, and clean the program inside a disposable guest through a pinned loader.

#### Scenario: BPF case matches

- GIVEN the verifier accepts the exact object, declared attachment succeeds, the bounded trigger produces the expected typed event, and detach/cleanup succeeds
- WHEN case classification runs
- THEN evidence MUST retain verifier, attach, observation, and cleanup classes separately
- AND it MUST bind the exact kernel/BTF/pack/object/case identities.

#### Scenario: Verification, attachment, or observation fails

- GIVEN BTF or a declared requirement is missing, the verifier rejects, the attach target differs, no expected event arrives, or detach/cleanup fails
- WHEN BPF classification runs
- THEN the required case MUST not pass
- AND bounded verifier diagnostics may be hashed or classified but raw unbounded logs MUST remain debug-only.

### Requirement: Kernel-bundle behavior evidence stays scoped

r[kernel_bundle_validation.evidence] ChaosControl MUST emit domain-separated BLAKE3 receipts with role `kernel-bundle/vm-compat-smoke` that bind exact Onix/Mantle/ChaosControl identities, requested cases, execution bounds, typed observations, terminal classes, and cleanup while excluding raw logs, credentials, host paths, component bytes, and overclaims.

#### Scenario: Complete behavior receipt is emitted

- GIVEN all required positive controls and cases reach their expected terminal classes
- WHEN receipt projection runs
- THEN it MAY report bounded VM compatibility smoke for that exact cohort
- AND it MUST NOT claim universal bootability, module/eBPF safety, build correctness, snapshot replay, physical compatibility, deployability, or release eligibility.

#### Scenario: Another evidence gate consumes the receipt broadly

- GIVEN a consumer attempts to use the receipt as lifecycle replay, snapshot-backed proof, physical readiness, security proof, build proof, or release evidence
- WHEN evidence-role validation runs
- THEN it MUST reject the scope promotion.

### Requirement: Wiring and KVM behavior rails remain distinct

r[kernel_bundle_validation.rails] ChaosControl MUST provide a cheap default rail for profile, fixture, adapter, protocol, identity, and non-claim wiring plus a separate opt-in KVM rail for actual boot/module/BPF behavior, and wiring or missing-prerequisite results MUST NOT count as behavior success.

#### Scenario: Wiring drift fails cheaply

- GIVEN a required contract, fixture, adapter, guest protocol, identity binding, or claim guard is removed
- WHEN the default wiring rail runs
- THEN it MUST fail without launching a VM.

#### Scenario: KVM is unavailable

- GIVEN the behavior rail is selected on a host without required KVM or guest loader support
- WHEN prerequisites run
- THEN it MUST report blocked evidence with remediation
- AND it MUST NOT reuse a wiring pass as boot, module, or BPF evidence.

### Requirement: Kernel-bundle validation has positive and negative evidence

r[kernel_bundle_validation.verification] The rail MUST include positive and negative profile, input-admission, boot, module, BPF, bound, cleanup, receipt, wiring, KVM, and evidence-role fixtures and MUST preserve current receipts for at least one selected boot, module, and BPF case before archive.

#### Scenario: Kernel-bundle rail is ready to archive

- GIVEN maintainers intend to close the kernel-bundle validation change
- WHEN closeout validation runs
- THEN focused pure/contract/guest/wiring checks, selected KVM behavior cases, dependency policy, docs, Cairn validation, and proposal/design/tasks gates MUST pass
- AND unavailable or failed behavior cases MUST remain explicit rather than being generalized from static evidence.
