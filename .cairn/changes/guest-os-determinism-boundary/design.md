# Design: Guest OS Determinism Boundary

## Context

The VMM already filters CPUID, pins the TSC, and provides deterministic virtio entropy, block, and network devices. The guest kernel builds through Nix, so its configuration is under the repo's control. Guest userspace reads outside the admitted surface still break replay.

## Decisions

### 1. Entropy is injected, not intercepted

The VMM feeds a run-derived deterministic stream into the kernel entropy sources during boot (for example through the deterministic virtio entropy device and an explicit early-entropy write). The kernel CRNG then produces reproducible user-space streams. This avoids a syscall interception layer for the first version.

### 2. Time is pinned, not virtualized per call

Guest clocks derive from the pinned TSC. The profile records the clock mapping and validates monotonic deltas rather than absolute wall values. Host RTC and wall-clock reads that bypass the virtual surface are declared out of profile.

### 3. Layout is seeded from the run

The guest receives a run-derived ASLR seed through the kernel command line or a fixed early write. All processes inherit it. The seed enters the receipt.

### 4. Signals follow the schedule

Signal delivery order derives from the deterministic vCPU schedule. The validation fixture exercises two signals and requires identical order across identical runs. No host-side signal mediation is added.

### 5. Validation proves the boundary

A fixture guest records bytes from every admitted surface. A drift gate requires bit-exact equality across repeated identical runs. The gate fails closed and names the drifting surface.

### 6. The boundary stays enumerated

The profile lists each admitted surface. Claims about unlisted reads are rejected. This keeps the guarantee narrow and honest.

## Risks

Full syscall interception, as Antithesis uses, remains out of scope. Interception is the only complete answer for arbitrary closed binaries that read time or entropy through unusual paths. This change makes the common surfaces reproducible and documents the rest as out of profile.
