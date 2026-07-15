## Context

The Onix kernel-bundle contract separates kernel-build, full bundle, ModulePack, BPF Pack, manifest, KBI, and OCI identities. Mantle can verify and materialize those bytes. Neither layer should load privileged artifacts while building or validating contracts.

ChaosControl can boot Linux and run deterministic guest workloads under KVM. The new rail uses that boundary for compatibility smoke only. It does not merge with the active OnixOS lifecycle fault/replay change, whose assertions concern staging, activation, persistence, rollback, and status behavior.

## Decisions

### 1. Require complete upstream evidence before launch

**Choice:** Input admission requires a valid `onix-kernel-bundle-v1` validation receipt, exact canonical manifest, matching Mantle OCI import/export or materialization receipt, present component/pack refs, and matching BLAKE3/SHA-256 role identities. It also requires a supported kernel format and guest-injection plan. Any mismatch stops before VMM creation.

**Rationale:** Runtime smoke cannot repair missing build, provenance, or transport evidence.

### 2. Author bounded campaigns in Nickel

**Choice:** A typed campaign binds profile schema, bundle/kernel/manifest/pack identities, guest harness and rootfs/initrd identity, boot format, command line, module/BPF cases, expected observations, deterministic seed, virtual-step/exit/memory/artifact bounds, retention, and non-claims. No unexplained limit is embedded in runner code.

**Rationale:** Hostile or broken privileged artifacts require strict reviewable resource limits.

### 3. Use a structured guest protocol

**Choice:** A small Rust guest harness receives a read-only case manifest and reports typed setup, boot-ready, module, BPF, trigger, cleanup, and final observations over the existing bounded SDK/protocol channel. Raw serial logs remain debug-only and cannot satisfy a verdict.

**Rationale:** Stable evidence should not depend on scraping kernel text or unbounded verifier logs.

### 4. Admit only supported boot formats

**Choice:** The initial cohort is x86_64 kernel formats already supported by ChaosControl/linux-loader plus an admitted initrd/rootfs shape. Compressed or protocol variants not supported by the VMM are classified blocked before launch, not silently converted through ambient host tools.

**Rationale:** “Kernel image” is not a universal executable format.

### 5. Prove a minimal boot oracle first

**Choice:** Boot success requires the guest harness to report its exact running kernel release/architecture, expected bundle/kernel identities supplied through the read-only case manifest, setup completion, and a bounded liveness observation. A no-fault control is required for every profile.

**Rationale:** Reaching arbitrary serial text is weaker than a workload-owned structured readiness boundary.

### 6. Exercise ModulePacks in disposable guests

**Choice:** Each module case binds pack/member identity, expected module name, target kernel facts, load parameters, and a bounded observation. The guest verifies the injected bytes, loads the module, observes the declared safe effect or metadata, unloads when supported, and records cleanup. Failure, taint, signature state, or inability to unload remains explicit.

**Rationale:** Vermagic and manifest agreement are not proof that the module loads or behaves as expected.

### 7. Exercise BPF Packs through declared cases

**Choice:** Each BPF case binds object/manifest/BTF identity, section, attach class/target, declared kfunc/type requirements, trigger action, and expected bounded event. The guest verifies bytes, invokes the pinned loader, records verifier outcome class and bounded digest, attaches, triggers, observes, detaches, and cleans maps/links. Arbitrary undeclared attach behavior is rejected.

**Rationale:** Compilation and manifest presence do not prove verifier acceptance or successful attachment.

### 8. Keep behavior classes narrow

**Choice:** Emit separate classes for input-blocked, boot-ready, boot-failed, module-loaded, module-observation-matched, module-rejected, module-cleanup-failed, BPF-verified, BPF-attached, BPF-observation-matched, BPF-rejected, BPF-cleanup-failed, bound-exceeded, harness-error, and unsupported. A profile passes only when all required positive controls and expected cases reach their exact terminal class.

**Rationale:** Collapsing every failure into “did not boot” or every load into “safe” would hide the tested boundary.

### 9. Separate wiring and KVM rails

**Choice:** A cheap default rail validates profiles, fixture identities, input adapter logic, guest protocol schemas, expected command anchors, and evidence non-claims without launching a VM. A dedicated KVM rail executes bounded positive and negative cases. Missing KVM or loader prerequisites produce blocked evidence.

**Rationale:** Ordinary checks stay portable while behavior claims require actual execution.

### 10. Preserve evidence-role separation

**Choice:** Receipts link exact Onix/Mantle identities, ChaosControl profile/harness/VMM cohort, observations, cleanup, and bounds with BLAKE3. They use a new `kernel-bundle/vm-compat-smoke` role and do not satisfy snapshot-backed replay, lifecycle fault evidence, physical readiness, security proof, build correctness, or release eligibility.

**Rationale:** Runtime compatibility smoke is useful only if its limits remain visible.

## Risks / Trade-offs

- Guest module/BPF loaders and rootfs composition add maintenance and can lag kernel features. Cohort pins and unsupported classifications keep drift explicit.
- A successful smoke can miss latent bugs or unsafe behavior outside the trigger. The evidence is case-specific, not a safety proof.
- Some modules cannot unload cleanly. Such cases need explicit terminal policy and fresh-VM isolation rather than pretending cleanup succeeded.
- KVM runners may be unavailable in generic CI; the cheap rail must not be promoted to behavior evidence.

## Non-Goals

- Rebuilding bundles, interpreting Onix inventory, or replacing Mantle admission.
- Loading modules or BPF on the host, bare metal, production machines, or operator kernels.
- Full kernel self-test, fuzzing every module/BPF path, performance benchmarking, signature trust, measured boot, or universal compatibility.
- Merging this evidence with OnixOS lifecycle-fault replay, snapshot-backed replay proof, or physical readiness.
- Proving KernelScript compiler/language correctness when a candidate came from the separate experiment.
