# Kernel-bundle KVM rail validation

- Date: 2026-07-15
- Question: Does the ChaosControl kernel-bundle validation rail now have an opt-in KVM shell that keeps verdicts structured and fail-closed when KVM or loader inputs are unavailable?
- Decision: **partial implementation, not archive-ready**. The CLI now has a `--kvm-run-profile <profile> --kernel <path> --initrd <path> --out <receipt>` shell backed by `chaoscontrol-vmm::DeterministicVm`, plus `--check-kvm-serial` and `--sample-kvm-markers` modes. A structured marker transcript can produce a passed KVM rail receipt, and missing loader inputs produce a blocked receipt without missing-marker noise. This still does not execute the exact Mantle private-kfunc module/BPF loader inside a guest, so final target authority remains blocked.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: build the exact guest initrd/loader that copies the selected Mantle module/BPF artifacts into a disposable guest, performs module load/unload and BPF verify/attach/detach/cleanup, then emits the same structured marker protocol from real KVM execution.

## Commands

```console
nix develop -c cargo fmt -p chaoscontrol-evidence
nix develop -c cargo test -p chaoscontrol-evidence kernel_bundle_validation
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --sample-profile > /tmp/chaos-kernel-bundle-kvm-cli/profile.json
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --sample-kvm-markers > /tmp/chaos-kernel-bundle-kvm-cli/serial.txt
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --check-kvm-serial /tmp/chaos-kernel-bundle-kvm-cli/profile.json /tmp/chaos-kernel-bundle-kvm-cli/serial.txt > /tmp/chaos-kernel-bundle-kvm-cli/receipt2.json
/home/brittonr/.cargo-target/debug/kernel-bundle-vm-compat-smoke --kvm-run-profile /tmp/chaos-kernel-bundle-kvm-cli/profile.json --kernel /tmp/chaos-kernel-bundle-kvm-cli/missing-vmlinux --initrd /tmp/chaos-kernel-bundle-kvm-cli/missing-initrd.gz --out /tmp/chaos-kernel-bundle-kvm-cli/blocked-receipt2.json
```

## Focused test result

From pueue task `20`:

```text
test kernel_bundle_validation::tests::cleanup_and_non_claim_gaps_cannot_pass ... ok
test kernel_bundle_validation::tests::stale_or_role_confused_inputs_fail_before_receipt ... ok
test kernel_bundle_validation::tests::exact_mantle_private_kfunc_profile_emits_scoped_receipt ... ok
test kernel_bundle_validation::tests::raw_log_or_missing_cleanup_cannot_pass_kvm_rail ... ok
test kernel_bundle_validation::tests::unavailable_kvm_is_blocked_not_passed ... ok
test kernel_bundle_validation::tests::kvm_markers_emit_passed_rail_receipt ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out; finished in 0.00s
```

Nonblocking Nix warnings during the same run:

```text
git: 'remote-https' is not a git command. See 'git --help'.
fatal: remote helper 'https' aborted session
warning: could not get HEAD ref for repository 'https://github.com/trailofbits/dylint'; using expired cached ref 'refs/heads/master'
```

## Marker-pass receipt excerpt

```text
"status": "passed"
"profile_identity_blake3": "216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589"
"receipt_identity_blake3": "ef38c2f41862b9a4c0cf3be09dd50290780004897b307278fc2f41c4380f9ee6"
```

The persisted receipt is `mantle-private-kfunc-kvm-marker-pass-2026-07-15.json`.

## Blocked-input receipt excerpt

```text
"status": "blocked"
"kvm_available": true
"loader_available": false
"receipt_identity_blake3": "c9798576d1425d456dd6a544c0e2b6d332347ee608e7771c7de0ae2c719cadab"
```

The persisted receipt is `mantle-private-kfunc-kvm-blocked-input-2026-07-15.json`.

## Non-claims

The marker-pass receipt is a structured-transcript classification test, not real KVM execution of the Mantle private-kfunc loader. The blocked-input receipt proves fail-closed shell behavior only. Neither receipt satisfies ChaosControl snapshot replay, Onix lifecycle replay, physical readiness, build correctness, security, or release gates.
