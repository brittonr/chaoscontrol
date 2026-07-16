# Kernel-bundle KVM rail validation

- Date: 2026-07-15
- Question: Does the ChaosControl kernel-bundle validation rail now have a repo-owned initrd/loader path that executes the exact Mantle private-kfunc module/BPF artifacts under KVM while keeping transcript-only and blocked-input receipts distinct?
- Decision: **implemented for the positive exact Mantle private-kfunc cohort, not archive-ready for the whole change**. The CLI now builds an uncompressed `newc` initrd from repo-owned code, injects BusyBox, bpftool, a repo-owned exact delete-module helper, required Nix closures, and the exact Mantle `private_kfunc.mod.ko`, `private_kfunc.ebpf.o`, and `private_kfunc` artifacts. The KVM rail hashes the actual kernel/initrd inputs into the receipt, executes the guest through `chaoscontrol-vmm::DeterministicVm`, and records structured boot/module/BPF/cleanup observations. Remaining open work is the broader negative fixture set and documentation.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: add the remaining stale digest, missing BTF/kfunc, verifier rejection, wrong attach target, cleanup failure, and guard/non-claim fixtures before archive.

## Commands

```console
nix develop -c cargo fmt -p chaoscontrol-evidence
nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_validation
nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_initrd
nix develop -c cargo test -p chaoscontrol-evidence --bin kernel-bundle-delete-module -- --list
/home/brittonr/.cargo-target/debug/kernel-bundle-delete-module
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --sample-profile > /tmp/chaos-kernel-bundle-exact/profile-current.json
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --sample-kvm-markers > /tmp/chaos-kernel-bundle-exact/kvm-markers-current.txt
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --check-kvm-serial /tmp/chaos-kernel-bundle-exact/profile-current.json /tmp/chaos-kernel-bundle-exact/kvm-markers-current.txt > cairn/changes/add-kernel-bundle-validation-rail/evidence/mantle-private-kfunc-kvm-marker-pass-2026-07-15.json
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --kvm-run-profile /tmp/chaos-kernel-bundle-exact/profile-current.json --kernel /tmp/chaos-kernel-bundle-exact/missing-kernel --initrd /tmp/chaos-kernel-bundle-exact/missing-initrd --out cairn/changes/add-kernel-bundle-validation-rail/evidence/mantle-private-kfunc-kvm-blocked-input-2026-07-15.json --max-exits 300000
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --build-private-kfunc-initrd /tmp/chaos-kernel-bundle-exact/private-kfunc-initrd-v7.cpio --artifacts-dir /nix/store/6cz7sqcq3mp7vnpz2nipcjx600b15fxv-mantle-kernelscript-production-artifacts/artifacts --busybox /nix/store/8mf4s8c4xjvlkj12p299qylrb30g7zzh-busybox-static-x86_64-unknown-linux-musl-1.37.0/bin/busybox --bpftool /nix/store/rcy0axk6gaw5q6r3r631bnp6ap351jrg-bpftools-6.18/bin/bpftool --delete-module-helper /home/brittonr/.cargo-target/debug/kernel-bundle-delete-module --closure-list /tmp/chaos-kernel-bundle-exact/closure-roots-with-helper.txt > /tmp/chaos-kernel-bundle-exact/initrd-summary-v7.json
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --kvm-run-profile /tmp/chaos-kernel-bundle-exact/profile.json --kernel /tmp/chaos-kernel-bundle-exact/onix-kernel-dev-dev/vmlinux --initrd /tmp/chaos-kernel-bundle-exact/private-kfunc-initrd-v7.cpio --out /tmp/chaos-kernel-bundle-exact/exact-kvm-receipt-v7.json --max-exits 300000 --memory-mib 1024
```

## Focused test results

From pueue task `16` after the final initrd cleanup:

```text
$ nix develop -c cargo fmt -p chaoscontrol-evidence --check
$ nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_validation
running 6 tests
test kernel_bundle_validation::tests::stale_or_role_confused_inputs_fail_before_receipt ... ok
test kernel_bundle_validation::tests::cleanup_and_non_claim_gaps_cannot_pass ... ok
test kernel_bundle_validation::tests::exact_mantle_private_kfunc_profile_emits_scoped_receipt ... ok
test kernel_bundle_validation::tests::raw_log_or_missing_cleanup_cannot_pass_kvm_rail ... ok
test kernel_bundle_validation::tests::unavailable_kvm_is_blocked_not_passed ... ok
test kernel_bundle_validation::tests::kvm_markers_emit_passed_rail_receipt ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out; finished in 0.00s

$ nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_initrd
running 4 tests
test kernel_bundle_initrd::tests::init_script_rejects_empty_inputs ... ok
test kernel_bundle_initrd::tests::init_script_contains_structured_private_kfunc_markers ... ok
test kernel_bundle_initrd::tests::newc_writer_records_regular_files_dirs_and_symlinks ... ok
test kernel_bundle_initrd::tests::closure_roots_reject_relative_paths ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out; finished in 0.01s

$ nix develop -c cargo test -p chaoscontrol-evidence --bin kernel-bundle-delete-module -- --list
Running unittests src/bin/kernel-bundle-delete-module.rs (...)
0 tests, 0 benchmarks
```

## Receipt excerpts

Structured marker-only classification is still a transcript rail, not exact KVM execution:

```text
"status": "passed"
"execution_mode": "serial-marker-transcript"
"kernel_image_blake3": null
"initrd_image_blake3": null
"receipt_identity_blake3": "3fa7cf844e3c815ab5d31adebce82072bc91b92c6f6985c263c47a9b1938c628"
```

Missing loader inputs remain fail-closed:

```text
"status": "blocked"
"execution_mode": "chaoscontrol-vmm-kvm"
"kvm_available": true
"loader_available": false
"receipt_identity_blake3": "59f0b3425fe465a95b38456c0af9ba8abcacdcec6e14e67e0f2373677dc23f60"
```

Exact repo-owned KVM execution of the selected Mantle private-kfunc artifacts passed:

```text
"status": "passed"
"execution_mode": "chaoscontrol-vmm-kvm"
"kernel_image_blake3": "223a6b61393b8956124a574d0fac00057fc45171dd7bb56a7711ca1a224de5d7"
"initrd_image_blake3": "9ac442589b7f9e35b610961e67e236461dab8150d5ec1c8139b8a43c9ae1a29a"
"receipt_identity_blake3": "b0273764265f5beea526aa56acbf5f723a0d193af1e54626c5bf0062e4856cb0"
"issues": []
```

Persisted files:

- `mantle-private-kfunc-kvm-marker-pass-2026-07-15.json`
- `mantle-private-kfunc-kvm-blocked-input-2026-07-15.json`
- `mantle-private-kfunc-initrd-summary-2026-07-15.json`
- `mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json`

## Non-claims

The exact KVM receipt proves only the bounded disposable-VM path for this selected Mantle private-kfunc cohort under the named ChaosControl runner inputs. It does not claim universal bootability, module safety, eBPF safety, build correctness, snapshot replay, Onix lifecycle replay, physical readiness, security, release eligibility, or production deployability. The marker-only receipt remains transcript classification, and the blocked-input receipt proves fail-closed shell behavior only.
