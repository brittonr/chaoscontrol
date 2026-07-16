# Kernel-bundle KVM rail validation

- Date: 2026-07-15
- Question: Does the ChaosControl kernel-bundle rail execute the exact Mantle private-kfunc cohort under KVM, reject stale input before VMM creation, exercise selected negative behavior inside disposable guests, and prevent transcript/evidence-role promotion?
- Decision: **yes for the selected bounded cohort**. One repo-owned deterministic initrd supports the positive case plus missing-kfunc, verifier-rejection, wrong-attach-target, and cleanup-failure scenarios. Expected and measured kernel/initrd BLAKE3 identities are mandatory. Stale digest input is blocked before VMM creation. Marker transcripts are now classified as failed exact-KVM evidence. Pure guards reject snapshot replay, Onix lifecycle replay, physical readiness, build correctness, security, and release use.
- Owner: ChaosControl kernel-bundle validation maintainers.
- Next action: sync and archive through Cairn; retain all receipts as bounded VM compatibility evidence only.

## Exact cohort

```text
profile_identity_blake3 = 216bd1a6c5461209f340a9c4f4d00aacf5c2312679bb9cb5808d329c619fc589
kernel_image_blake3 = 223a6b61393b8956124a574d0fac00057fc45171dd7bb56a7711ca1a224de5d7
initrd_image_blake3 = 48bd470f32f96bc26d3d2599f1ab0dba4b3c2dac6eab658bcbce382e21d8c9e8
module_blake3 = 1a738476dabe13e3d8ae2c5b0435f7b7f2908a82fadcee136e5494f6a93a81e1
bpf_object_blake3 = b8cdd1315b4066c053a14034344a1b051f85fe2c965cffdc38d79d116ebb94de
```

The generated initrd summary records schema version `2`, 55 closure roots, the exact three Mantle artifact inputs, and the five guest scenarios.

## Reproduction shape

The full operator procedure is documented in `docs/kernel-bundle-validation.md`. The evidence run built `kernel-bundle-vm-compat-smoke` and `kernel-bundle-delete-module`, generated one uncompressed `newc` initrd, then invoked:

```console
kernel-bundle-vm-compat-smoke \
  --kvm-run-profile profile.json \
  --kernel "$VMLINUX" \
  --initrd private-kfunc-initrd.cpio \
  --expected-kernel-blake3 "$VMLINUX_BLAKE3" \
  --expected-initrd-blake3 "$INITRD_BLAKE3" \
  --scenario "$SCENARIO" \
  --memory-mib 1024 \
  --max-exits 300000 \
  --out "$RECEIPT"
```

## Positive receipt

`mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json`:

```text
status = passed
execution_mode = chaoscontrol-vmm-kvm
scenario = positive
negative_fixture_matched = false
issues = []
receipt_identity_blake3 = 40f624ff0ff51e46bbab3813a4122ff5329be9019c1a4d73f44d11cb242daae8
```

The receipt records exact boot readiness, module load/unload/cleanup, and BPF verify/attach/detach/cleanup observations.

## Negative receipts

| Scenario | Terminal status | Exact negative match | Failure class | Receipt BLAKE3 |
|---|---|---:|---|---|
| stale digest | blocked | true | `input-digest-mismatch` before VMM creation | `e1c33944d33527b4335e6d675157c5029808bc0e335fc19ed2fddbf68f70e952` |
| missing kfunc | failed | true | `guest-error:bpf:missing-kfunc-rejected` | `cfea5752758450bef4cdbbec306a6354958d25e740a5fc9889f4945f7d4ef605` |
| verifier rejection | failed | true | `guest-error:bpf:verifier-rejected` | `1df978667e04174efad796a09db3866c790d0267b30c47df29d2aa4985cbd86d` |
| wrong attach target | failed | true | `guest-error:bpf:wrong-attach-target-rejected` | `b637120e3d449375c1b31bc70e744ea425e3915e8a7515d62b31f6a3cd138bd3` |
| cleanup failure | failed | true | `guest-error:bpf:cleanup-failed` | `bc36de1ea37a6d0e23df5561778f2291b356078e57e127a80b5c0365e526c166` |

The wrong-target and cleanup scenarios first reach the expected earlier positive observations, then fail at the selected boundary. Each scenario runs in a fresh disposable VM.

## Transcript and unavailable-input guards

- `mantle-private-kfunc-kvm-marker-transcript-rejected-2026-07-15.json` records `execution-mode-not-exact-kvm`; structured text is parser evidence, not behavior success.
- `mantle-private-kfunc-kvm-blocked-input-2026-07-15.json` records missing kernel/initrd inputs as blocked.
- `kernel_bundle_receipt_supports_use` accepts only an issue-free exact-KVM positive receipt for the narrow VM-compatibility-smoke use and denies all broader evidence roles.

## Focused validation

From pueue task `99`:

```text
$ nix develop -c cargo fmt -p chaoscontrol-evidence --check
$ nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_validation
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out

$ nix develop -c cargo test -p chaoscontrol-evidence --lib kernel_bundle_initrd
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 31 filtered out

$ nix develop -c cargo clippy -p chaoscontrol-evidence --lib --bin kernel-bundle-vm-compat-smoke --no-deps -- -D warnings
Finished `dev` profile
```

The negative matrix also covers unsupported architecture/release, profile bounds, VM-exit overflow, panic/no-readiness, module vermagic/signature/rejection/taint/unload classes, absent BTF/type/event classes, raw-log-only input, cleanup gaps, stale role/digest inputs, and unavailable KVM/loaders in pure deterministic tests.

Closeout checks:

```text
pueue task 25: dependency-audit passed with vulnerabilities=0 and no untriaged warnings
pueue task 25: dependency-policy passed
pueue task 19: canonical-policy validate plus proposal/design/tasks gates passed
```

Canonical policy: `/home/brittonr/git/OnixResearch/cairn/cairn-policy/generated/cairn-policy.json`.

## Persisted files

- `mantle-private-kfunc-materialization-2026-07-15.json`
- `mantle-private-kfunc-initrd-summary-2026-07-15.json`
- `mantle-private-kfunc-exact-kvm-receipt-2026-07-15.json`
- `mantle-private-kfunc-kvm-negative-stale-digest-2026-07-15.json`
- `mantle-private-kfunc-kvm-negative-missing-kfunc-2026-07-15.json`
- `mantle-private-kfunc-kvm-negative-verifier-rejection-2026-07-15.json`
- `mantle-private-kfunc-kvm-negative-wrong-attach-target-2026-07-15.json`
- `mantle-private-kfunc-kvm-negative-cleanup-failure-2026-07-15.json`
- `mantle-private-kfunc-kvm-marker-transcript-rejected-2026-07-15.json`
- `mantle-private-kfunc-kvm-blocked-input-2026-07-15.json`

## Non-claims

This proves selected bounded disposable-VM compatibility behavior for one exact cohort only. It does not prove universal bootability, module/eBPF safety, kernel correctness, build correctness, deterministic replay, Onix lifecycle behavior, physical readiness, security, deployability, or release eligibility.
