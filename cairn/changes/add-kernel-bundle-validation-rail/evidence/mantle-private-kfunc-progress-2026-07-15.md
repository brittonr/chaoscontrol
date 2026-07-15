# Mantle private-kfunc kernel-bundle validation progress

- Date: 2026-07-15
- Question: Can ChaosControl bind the exact Mantle KernelScript private-kfunc cohort to a scoped `kernel-bundle/vm-compat-smoke` receipt without promoting static or Mantle-only evidence to general safety?
- Decision: **partial implementation, not archive-ready**. `chaoscontrol-evidence::kernel_bundle_validation` now defines a pure profile validator and receipt projector for one exact Onix/Mantle cohort. The receipt role is `kernel-bundle/vm-compat-smoke`, uses BLAKE3 identities, binds Onix bundle/manifest/module/BPF pack identities, Mantle module/BPF bytes, the observed disposable VM smoke status, terminal module/BPF observations, cleanup classes, bounds, and non-claims. Positive and negative unit tests pass. The dedicated ChaosControl KVM execution rail remains incomplete, so the change stays active.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: replace the imported external NixOS VM smoke runner with the dedicated ChaosControl KVM rail, then add the remaining negative behavior fixtures.

## Exact identities

```text
onix.kernel_build_identity = onix:blake3:kernel-build:4ee8064c7daf33498bd61d85d573c28b43febf54926bfe1e58ef5df76637e0c2
onix.module_pack_identity = onix:blake3:module-pack:b06089102d69299754550d55ea23d40b3235b2be010242a2a62c6de1d3aafcef
onix.bpf_pack_identity = onix:blake3:bpf-pack:e63907102511d66cc006163e9e96e15b0e89e758a6843ab4d235faafc0eebb6a
mantle.module_blake3 = 1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1
mantle.bpf_object_blake3 = b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de
chaoscontrol.profile_identity_blake3 = 216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589
chaoscontrol.receipt_identity_blake3 = fb37d05d6ee328b05d8f1bdc80ae0d622dcdef590f0dbf7e2721bb3993e76119
```

Committed evidence files:

- `evidence/mantle-private-kfunc-onix-validation-2026-07-15.json`
- `evidence/mantle-private-kfunc-profile-2026-07-15.json`
- `evidence/mantle-private-kfunc-vm-compat-smoke-2026-07-15.json`

## Focused validation

Command:

```console
nix develop -c cargo fmt -p chaoscontrol-evidence
nix develop -c cargo test -p chaoscontrol-evidence kernel_bundle_validation
```

Result (pueue task `40`):

```text
test kernel_bundle_validation::tests::stale_or_role_confused_inputs_fail_before_receipt ... ok
test kernel_bundle_validation::tests::cleanup_and_non_claim_gaps_cannot_pass ... ok
test kernel_bundle_validation::tests::exact_mantle_private_kfunc_profile_emits_scoped_receipt ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out
```

Focused Cairn tasks gate with the canonical generated policy also passed during this update:

```text
"verdict": "PASS"
"task_done": 5
"task_todo": 13
```

The CLI self-check was rerun with the compiled binary because the Nix devshell banner writes to stdout:

```console
kernel-bundle-vm-compat-smoke --check-profile evidence/mantle-private-kfunc-profile-2026-07-15.json
```

and emitted a receipt matching `mantle-private-kfunc-vm-compat-smoke-2026-07-15.json`.

## Non-claims

This evidence does not claim universal bootability, module safety, eBPF safety, build correctness, snapshot replay, ChaosControl VMM determinism, physical readiness, production deployability, or release eligibility. The dedicated KVM behavior rail remains the remaining product blocker.
