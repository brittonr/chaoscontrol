# Mantle private-kfunc kernel-bundle validation progress

- Date: 2026-07-15
- Question: Can ChaosControl bind the exact Mantle KernelScript private-kfunc cohort to a scoped `kernel-bundle/vm-compat-smoke` receipt without promoting static or Mantle-only evidence to general safety?
- Decision: **positive exact KVM cohort implemented, not archive-ready for the full change**. `chaoscontrol-evidence::kernel_bundle_validation` defines the pure profile validator and receipt projector for one exact Onix/Mantle cohort. The CLI now also builds a repo-owned private-kfunc initrd, injects the exact Mantle module/BPF/loader artifacts and required closures, boots the selected Onix-pinned `vmlinux` through `chaoscontrol-vmm::DeterministicVm`, verifies/attaches/detaches BPF, unloads the dotted module name with a repo-owned syscall helper, and emits a digest-bound exact KVM receipt. Positive and negative unit tests pass. The change stays active for remaining negative behavior fixtures and documentation.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: add stale digest, missing BTF/kfunc, verifier rejection, wrong attach target, cleanup failure, guard/non-claim fixtures, and reproduction docs before archive.

## Exact identities

```text
onix.kernel_build_identity = onix:blake3:kernel-build:4ee8064c7daf33498bd61d85d573c28b43febf54926bfe1e58ef5df76637e0c2
onix.module_pack_identity = onix:blake3:module-pack:b06089102d69299754550d55ea23d40b3235b2be010242a2a62c6de1d3aafcef
onix.bpf_pack_identity = onix:blake3:bpf-pack:e63907102511d66cc006163e9e96e15b0e89e758a6843ab4d235faafc0eebb6a
mantle.module_blake3 = 1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1
mantle.bpf_object_blake3 = b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de
chaoscontrol.profile_identity_blake3 = 216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589
chaoscontrol.receipt_identity_blake3 = fb37d05d6ee328b05d8f1bdc80ae0d622dcdef590f0dbf7e2721bb3993e76119
chaoscontrol.kvm_marker_pass_receipt_identity_blake3 = 3fa7cf844e3c815ab5d31adebce82072bc91b92c6f6985c263c47a9b1938c628
chaoscontrol.kvm_blocked_input_receipt_identity_blake3 = 59f0b3425fe465a95b38456c0af9ba8abcacdcec6e14e67e0f2373677dc23f60
chaoscontrol.exact_kvm_receipt_identity_blake3 = b0273764265f5beea526aa56acbf5f723a0d193af1e54626c5bf0062e4856cb0
chaoscontrol.exact_kvm_kernel_image_blake3 = 223a6b61393b8956124a574d0fac00057fc45171dd7bb56a7711ca1a224de5d7
chaoscontrol.exact_kvm_initrd_image_blake3 = 9ac442589b7f9e35b610961e67e236461dab8150d5ec1c8139b8a43c9ae1a29a
```

Committed evidence files:

- `evidence/mantle-private-kfunc-onix-validation-2026-07-15.json`
- `evidence/mantle-private-kfunc-profile-2026-07-15.json`
- `evidence/mantle-private-kfunc-vm-compat-smoke-2026-07-15.json`
- `evidence/mantle-private-kfunc-kvm-marker-pass-2026-07-15.json`
- `evidence/mantle-private-kfunc-kvm-blocked-input-2026-07-15.json`
- `evidence/kvm-rail-validation-2026-07-15.md`
- `evidence/mantle-private-kfunc-initrd-summary-2026-07-15.json`
- `evidence/mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json`
- `evidence/post-exact-kvm-cairn-validation-2026-07-15.md`

## Focused validation

Command:

```console
nix develop -c cargo fmt -p chaoscontrol-evidence
nix develop -c cargo test -p chaoscontrol-evidence kernel_bundle_validation
```

Result (pueue task `20` after adding the KVM rail shell):

```text
test kernel_bundle_validation::tests::cleanup_and_non_claim_gaps_cannot_pass ... ok
test kernel_bundle_validation::tests::stale_or_role_confused_inputs_fail_before_receipt ... ok
test kernel_bundle_validation::tests::exact_mantle_private_kfunc_profile_emits_scoped_receipt ... ok
test kernel_bundle_validation::tests::raw_log_or_missing_cleanup_cannot_pass_kvm_rail ... ok
test kernel_bundle_validation::tests::unavailable_kvm_is_blocked_not_passed ... ok
test kernel_bundle_validation::tests::kvm_markers_emit_passed_rail_receipt ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out; finished in 0.00s
```

Focused initrd-builder tests also passed:

```text
running 4 tests
test kernel_bundle_initrd::tests::init_script_rejects_empty_inputs ... ok
test kernel_bundle_initrd::tests::init_script_contains_structured_private_kfunc_markers ... ok
test kernel_bundle_initrd::tests::closure_roots_reject_relative_paths ... ok
test kernel_bundle_initrd::tests::newc_writer_records_regular_files_dirs_and_symlinks ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 0.00s
```

The repo-owned delete-module helper binary compiled and fails closed without a module name:

```text
0 tests, 0 benchmarks
exit_status=1
kernel-bundle-delete-module: usage: kernel-bundle-delete-module <module-name>
```

The exact KVM run produced `mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json` with `execution_mode = chaoscontrol-vmm-kvm`, status `passed`, and no issues. Post exact-KVM Cairn validation also passed; the tasks gate reported `task_done = 10`, `task_todo = 8`, `valid = true`, and `verdict = PASS` in `post-exact-kvm-cairn-validation-2026-07-15.md`.

## Non-claims

This evidence does not claim universal bootability, module safety, eBPF safety, build correctness, snapshot replay, Onix lifecycle replay, physical readiness, production deployability, security, or release eligibility. The structured marker-pass receipt is not a real guest execution. The blocked-input receipt proves fail-closed shell behavior only. The exact selected KVM positive rail is implemented; negative behavior fixtures remain the product blocker.
