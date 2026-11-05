# Guest OS determinism boundary

ChaosControl has one bounded Linux guest profile for four observed surfaces:

- run-derived boot-seed injection and snapshot-backed `getrandom` output;
- monotonic jiffies deltas;
- process text, stack, and heap layout;
- queued guest signal order.

The profile does not make a universal Linux or binary replay claim.

## Core and shell boundary

`chaoscontrol-sim-core::guest_determinism` owns the deterministic profile, BLAKE3 seed derivation, Linux seed-node bytes, probe validation, and drift decision. It has no KVM, file, process, clock, or entropy authority.

The VMM shell applies the plan. It writes a Linux x86 `SETUP_RNG_SEED` node before boot. It also supplies a seeded virtio RNG. The admitted gate hides TSC and uses deterministic jiffies from the VMM timer plan.

The first layout profile disables randomization. It uses `nokaslr`, `norandmaps`, and `randomize_kstack_offset=off`. The recorded layout binding selects that policy and its run inputs. It is not a claim that Linux accepts a caller-selected ASLR seed.

The dedicated guest fixture reaches a stable marker before it reads the four surfaces. `guest-determinism-gate` captures one complete VM snapshot, runs the probe, restores that snapshot, and runs the probe again. It accepts only when every observed field is bit-exact. Its receipt names each drifting surface.

Independent fresh boots are not byte-exact for Linux CRNG output because the kernel mixes additional boot observations. The profile records this limit instead of promoting fresh-boot equality.

## Run the gate

Build the kernel, fixture initrd, and host runner:

```bash
nix build .#determinism-probe-vmlinux .#initrd-determinism-probe .#default --no-link -L
```

Run two identical fixture guests:

```bash
kernel="$(nix path-info .#determinism-probe-vmlinux)/vmlinux"
initrd="$(nix path-info .#initrd-determinism-probe)"
nix run .#guest-determinism-gate -- \
  "$kernel" "$initrd" target/guest-determinism-receipt.json 47
```

The host must provide the admitted KVM capabilities. A build-only result does not replace the KVM receipt.

## Receipt contract

`contracts/guest-determinism/drift-receipt.ncl` validates the receipt shape. The contract requires agreement between `accepted` and the drift list. Positive and negative fixtures remain next to the contract.

## Guest time model

The default virtual TSC advances per VM exit or scheduling quantum. The scheduler's
guest-only retired-instruction progress is a candidate alternative source, but that
mode is not admitted: `CpuConfig::admit_tsc_mode` rejects
`instruction-progress` while PMU support reports no claim and while the runtime is
unimplemented. The pure snapshot model records mode, base, scale, and last retired
count and rejects cross-mode restores and retired-count regression. HLT decisions
keep the PIT-deadline jump and mark progress-mode jumps as declared bounds; PIT
programming uses the same configured `tsc_khz` in both modes.

Measured on 2026-11-05: AMD Ryzen 9 9950X3D, family 26 model 68 stepping 0, Linux
6.18.50, `perf_event_paranoid=-1`. Five 500-exit continuations from one snapshot were
1990/2000 bit-exact with p99 delta 0 and maximum absolute delta 6. The design gate
rejects this host profile for instruction-progress mode, so the default remains
unchanged and no PMU support is promoted.

## Claim limits

The receipt proves only the compared snapshot continuations, profile, seed binding, and four observed surfaces. It does not prove:

- equality across independent fresh Linux boots;
- host-side or cross-machine replay;
- behavior for reads outside the admitted list;
- arbitrary closed-binary determinism;
- syscall interception;
- host signal timing;
- Linux, KVM, or hardware correctness.
