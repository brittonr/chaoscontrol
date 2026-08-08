# Mantle private-kfunc kernel-bundle validation progress

- Date: 2026-07-15
- Question: Can ChaosControl bind the exact Mantle KernelScript private-kfunc cohort to scoped positive and negative `kernel-bundle/vm-compat-smoke` evidence without promoting static, transcript, or runtime-smoke evidence?
- Decision: **the selected ChaosControl KVM rail is implemented and validated**. The repo-owned initrd executes the exact positive module/BPF path and four selected guest failure scenarios under `chaoscontrol-vmm::DeterministicVm`; stale image identity is blocked before VMM creation. Expected and measured image identities, scenario, failure class, and exact negative match are receipt inputs. Transcript-only markers cannot pass. Broader evidence roles are denied by a pure guard.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: complete Cairn sync/archive after all focused and lifecycle gates are current.

## Exact identities

```text
onix.kernel_build_identity = onix:blake3:kernel-build:4ee8064c7daf33498bd61d85d573c28b43febf54926bfe1e58ef5df76637e0c2
onix.module_pack_identity = onix:blake3:module-pack:b06089102d69299754550d55ea23d40b3235b2be010242a2a62c6de1d3aafcef
onix.bpf_pack_identity = onix:blake3:bpf-pack:e63907102511d66cc006163e9e96e15b0e89e758a6843ab4d235faafc0eebb6a
mantle.module_blake3 = 1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1
mantle.bpf_object_blake3 = b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de
chaoscontrol.profile_identity_blake3 = 216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589
chaoscontrol.exact_kvm_kernel_image_blake3 = 223a6b61393b8956124a574d0fac00057fc45171dd7bb56a7711ca1a224de5d7
chaoscontrol.exact_kvm_initrd_image_blake3 = 48bd470f32f96bc26d3d2599f1ab0dba4b3c2dac6eab658bcbce382e21d8c9e8
chaoscontrol.exact_kvm_receipt_identity_blake3 = 40f624ff0ff51e46bbab3813a4122ff5329be9019c1a4d73f44d11cb242daae8
chaoscontrol.stale_digest_receipt_identity_blake3 = e1c33944d33527b4335e6d675157c5029808bc0e335fc19ed2fddbf68f70e952
chaoscontrol.missing_kfunc_receipt_identity_blake3 = cfea5752758450bef4cdbbec306a6354958d25e740a5fc9889f4945f7d4ef605
chaoscontrol.verifier_rejection_receipt_identity_blake3 = 1df978667e04174efad796a09db3866c790d0267b30c47df29d2aa4985cbd86d
chaoscontrol.wrong_attach_target_receipt_identity_blake3 = b637120e3d449375c1b31bc70e744ea425e3915e8a7515d62b31f6a3cd138bd3
chaoscontrol.cleanup_failure_receipt_identity_blake3 = bc36de1ea37a6d0e23df5561778f2291b356078e57e127a80b5c0365e526c166
```

## Evidence boundary

The authoritative session summary is `evidence/kvm-rail-validation-2026-07-15.md`. It links:

- exact Onix validation and Mantle materialization fixtures;
- initrd construction summary;
- exact positive KVM receipt;
- stale digest, missing kfunc, verifier rejection, wrong attach target, and cleanup failure receipts;
- transcript-rejection and unavailable-input receipts;
- focused test/clippy evidence; and
- the reproduction guide at `docs/kernel-bundle-validation.md`.

## Focused validation

```text
kernel_bundle_validation: 13 passed; 0 failed
kernel_bundle_initrd: 4 passed; 0 failed
focused chaoscontrol-evidence clippy: passed with -D warnings
```

The negative fixture matrix also covers unsupported architecture/release, bounds, panic/no-readiness, module vermagic/signature/rejection/taint/unload classes, missing BTF/type/event classes, role/digest drift, raw-log-only input, cleanup gaps, and unavailable prerequisites.

## Non-claims

This evidence does not claim universal bootability, module safety, eBPF safety, kernel correctness, build correctness, snapshot replay, Onix lifecycle replay, physical readiness, production deployability, security, or release eligibility. The exact positive receipt is bounded compatibility smoke only. Failed and blocked receipts prove precise rejection behavior, not positive compatibility.
