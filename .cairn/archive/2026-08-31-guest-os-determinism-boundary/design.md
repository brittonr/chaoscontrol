# Design: Guest OS Determinism Boundary

## Context

The VMM already filters CPUID, pins the TSC, and provides deterministic virtio entropy, block, and network devices. The guest kernel builds through Nix, so its configuration is under the repo's control. Guest userspace reads outside the admitted surface still break replay.

## Decisions

### 1. Entropy is injected, not intercepted

The VMM derives an early-boot seed from the run seed, VM identity, vCPU count, and clock profile. It writes that value through the Linux x86 `SETUP_RNG_SEED` boot-protocol node. The seeded virtio entropy device supplies the later stream. The profile disables trusted CPU and bootloader entropy inputs.

Fresh Linux boots still mix kernel timing and device observations into the CRNG. Therefore, the accepted byte-exact claim starts from one admitted quiescent snapshot. Two continuations from that snapshot must return identical `getrandom` bytes. Fresh-boot CRNG equality remains outside the profile. This avoids a syscall interception layer without hiding the observed fresh-boot drift.

### 2. The admitted clock uses deterministic jiffies

Fresh single-vCPU TSC reads drift while a guest executes between VM exits. The admitted profile therefore hides TSC and uses jiffies driven by the deterministic VMM PIT plan. It validates monotonic deltas rather than absolute wall values. Host RTC, wall-clock reads, and direct TSC reads are outside this profile.

### 3. The first layout profile disables randomization

Linux does not expose a supported caller-selected ASLR seed through the kernel command line. The first profile therefore uses `nokaslr`, `norandmaps`, and `randomize_kstack_offset=off`. A BLAKE3 layout binding records the run configuration and exact fixed-layout policy. The profile does not claim seeded Linux ASLR.

### 4. Guest signal order is observed under the schedule

The validation fixture queues two guest signals and requires identical delivery order across identical runs. The serialized vCPU schedule owns guest progress. No host-side signal mediation or host signal-timing claim is added.

### 5. Validation proves the snapshot-backed boundary

A fixture guest reaches a stable marker before it reads any admitted surface. The shell captures one complete VM snapshot, runs the fixture, restores the snapshot, and runs it again. A drift gate requires bit-exact equality across both continuations. The gate fails closed and names the drifting surface.

### 6. The boundary stays enumerated

The profile lists each admitted surface. Claims about unlisted reads are rejected. This keeps the guarantee narrow and honest.

## Risks

Full syscall interception, as Antithesis uses, remains out of scope. Interception is the only complete answer for arbitrary closed binaries that read time or entropy through unusual paths. This change makes the common surfaces reproducible and documents the rest as out of profile.
